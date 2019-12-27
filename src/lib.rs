//! Rust🦀 client for Apollo.
//!
//! Power by Rust `async/await`.
//!
use futures::future::try_join_all;
use http::StatusCode;
use isahc::ResponseExt;
use isahc::{get_async, HttpClientBuilder};
use lazy_static::lazy_static;
use quick_error::quick_error;
use serde::de::DeserializeOwned;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::{Debug, Display};
use std::ops::{Deref, DerefMut};
use std::time::Duration;
use std::{fmt, io};

#[cfg(test)]
mod tests;

/// Should be longer than server side's long polling timeout, which is now 60 seconds.
const DEFAULT_LISTEN_TIMEOUT: Duration = Duration::from_secs(90);

/// Apollo client crate side `Result`.
pub type ApolloClientResult<T> = Result<T, ApolloClientError>;

quick_error! {
    /// Apollo client crate side `Error`.
    #[derive(Debug)]
    pub enum ApolloClientError {
        Io(err: io::Error) {
            from()
            description("io error")
            display("I/O error: {}", err)
            cause(err)
        }
        Isahc(err: isahc::Error) {
            from()
            description("isahc error")
            display("Isahc error: {}", err)
            cause(err)
        }
        SerdeJson(err: serde_json::error::Error) {
            from()
            description("serde json error")
            display("Serde json error: {}", err)
            cause(err)
        }
        SerdeUrlencodedSer(err: serde_urlencoded::ser::Error) {
            from()
            description("serde urlencoded ser error")
            display("Serde urlencoded ser error: {}", err)
            cause(err)
        }
        #[cfg(feature = "yaml")]
        SerdeYaml(err: serde_yaml::Error) {
            description("serde yaml error")
            display("Serde yaml error: {}", err)
            cause(err)
        }
        #[cfg(feature = "xml")]
        SerdeXml(err: serde_xml_rs::Error) {
            description("serde xml error")
            display("Serde xml error: {}", err)
            cause(err)
        }
        EmptyResponses {
            description("empty responses")
            display("Empty responses")
        }
        UnknownApolloConfigurationKind(kind: &'static str) {
            description("unknown apollo configuration kind")
            display("Unknown apollo configuration kind: {}", kind)
        }
        ApolloContentNotFound {
            description("apollo content not found")
            display("Apollo content not found")
        }
        ApolloConfigNotFound {
            description("apollo config not found")
            display("Apollo config not found")
        }
        ApolloServerError {
            description("apollo server error")
            display("Apollo server error")
        }
        ApolloNotModified {
            description("apollo not modified")
            display("Apollo not modified")
        }
        ApolloOtherError(code: StatusCode) {
            description("apollo other error")
            display("apollo other error, status code: {}", code)
        }
    }
}

#[cfg(feature = "yaml")]
impl From<serde_yaml::Error> for ApolloClientError {
    fn from(err: serde_yaml::Error) -> ApolloClientError {
        ApolloClientError::SerdeYaml(err)
    }
}

#[cfg(feature = "xml")]
impl From<serde_xml_rs::Error> for ApolloClientError {
    fn from(err: serde_xml_rs::Error) -> ApolloClientError {
        ApolloClientError::SerdeXml(err)
    }
}

/// Configuration of Apollo and api information.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct ClientConfig<'a> {
    pub config_server_url: &'a str,
    pub app_id: &'a str,
    pub cluster_name: &'a str,
    pub namespace_names: Vec<&'a str>,
    #[serde(default)]
    pub ip: Option<IpValue<'a>>,
}

impl Default for ClientConfig<'_> {
    fn default() -> Self {
        Self {
            config_server_url: "http://localhost:8080",
            app_id: "",
            cluster_name: "default",
            namespace_names: vec!["application"],
            ip: Default::default(),
        }
    }
}

/// Apollo config api `ip` param value.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum IpValue<'a> {
    /// Get the hostname of the machine.
    HostName,
    /// Specify your own IP address or other text.
    Custom(&'a str),
}

impl<'a> IpValue<'a> {
    fn to_str(&'a self) -> &'a str {
        match self {
            IpValue::HostName => {
                lazy_static! {
                    static ref HOSTNAME: String = {
                        hostname::get()
                            .map_err(|_| ())
                            .and_then(|hostname| hostname.into_string().map_err(|_| ()))
                            .unwrap_or_else(|_| "unknown".to_string())
                    };
                }
                &HOSTNAME
            }
            IpValue::Custom(s) => s,
        }
    }
}

/// For apollo config api response to transfer to your favorite type.
pub trait FromResponses: Sized {
    type Err;

    fn from_responses(responses: Vec<Response>) -> Result<Self, Self::Err>;
}

impl FromResponses for Response {
    type Err = ApolloClientError;

    fn from_responses(responses: Vec<Response>) -> Result<Self, Self::Err> {
        Ok(responses
            .into_iter()
            .nth(0)
            .ok_or(ApolloClientError::EmptyResponses)?)
    }
}

impl FromResponses for Vec<Response> {
    type Err = ApolloClientError;

    fn from_responses(responses: Vec<Response>) -> Result<Self, Self::Err> {
        Ok(responses)
    }
}

impl FromResponses for HashMap<String, Response> {
    type Err = ApolloClientError;

    fn from_responses(responses: Vec<Response>) -> Result<Self, Self::Err> {
        let mut m = HashMap::with_capacity(responses.len());
        for response in responses {
            m.insert(response.namespace_name.clone(), response);
        }
        Ok(m)
    }
}

impl<T: DeserializeOwned> FromResponses for Configuration<T> {
    type Err = ApolloClientError;

    fn from_responses(responses: Vec<Response>) -> Result<Self, Self::Err> {
        Response::from_responses(responses)?.deserialize_to_configuration()
    }
}

impl<T: DeserializeOwned> FromResponses for Vec<Configuration<T>> {
    type Err = ApolloClientError;

    fn from_responses(responses: Vec<Response>) -> Result<Self, Self::Err> {
        responses
            .into_iter()
            .map(|response| response.deserialize_to_configuration())
            .collect()
    }
}

impl<T: DeserializeOwned> FromResponses for HashMap<String, Configuration<T>> {
    type Err = ApolloClientError;

    fn from_responses(responses: Vec<Response>) -> Result<Self, Self::Err> {
        <HashMap<String, Response>>::from_responses(responses)?
            .into_iter()
            .map(|(key, response)| {
                response
                    .deserialize_to_configuration()
                    .map(|configuration| (key, configuration))
            })
            .collect()
    }
}

/// The wrapper of apollo config api response's `configurations` field.
pub struct Configuration<T> {
    inner: T,
}

impl<T> Configuration<T> {
    pub fn new(inner: T) -> Self {
        Self { inner }
    }

    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T> Deref for Configuration<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> DerefMut for Configuration<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<T: Debug> Debug for Configuration<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        Debug::fmt(&format!("Configuration {{ {:?} }}", &self.inner), f)
    }
}

/// Kind of a configuration namespace.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConfigurationKind {
    Properties,
    Xml,
    Json,
    Yaml,
    Txt,
}

impl Display for ConfigurationKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        Display::fmt(
            match self {
                ConfigurationKind::Properties => "properties",
                ConfigurationKind::Xml => "xml",
                ConfigurationKind::Json => "json",
                ConfigurationKind::Yaml => "yaml",
                ConfigurationKind::Txt => "txt",
            },
            f,
        )
    }
}

/// Apollo config api response.
#[derive(Debug, Deserialize)]
pub struct Response {
    #[serde(rename = "appId")]
    pub app_id: String,
    pub cluster: String,
    #[serde(rename = "namespaceName")]
    pub namespace_name: String,
    pub configurations: HashMap<String, String>,
    #[serde(rename = "releaseKey")]
    pub release_key: String,
}

impl Response {
    /// Get the `configurations.content` field of the response.
    pub fn get_configurations_content(&self) -> ApolloClientResult<&str> {
        self.configurations
            .get("content")
            .map(|s| s.as_str())
            .ok_or(ApolloClientError::ApolloContentNotFound)
    }

    /// Infer the configuration namespace kind.
    pub fn infer_kind(&self) -> ConfigurationKind {
        let namespace_name = &self.namespace_name;

        if namespace_name.ends_with(".xml") {
            ConfigurationKind::Xml
        } else if namespace_name.ends_with(".json") {
            ConfigurationKind::Json
        } else if namespace_name.ends_with(".yml") || namespace_name.ends_with(".yaml") {
            ConfigurationKind::Yaml
        } else if namespace_name.ends_with(".txt") {
            ConfigurationKind::Txt
        } else {
            ConfigurationKind::Properties
        }
    }

    /// Deserialize the `configurations` field for `properties`, or `configurations.content` for
    /// other namespace kind, without wrapper.
    pub fn deserialize_configuration<T: DeserializeOwned>(&self) -> ApolloClientResult<T> {
        match self.infer_kind() {
            ConfigurationKind::Properties => {
                let object = serde_json::Value::Object(
                    self.configurations
                        .iter()
                        .map(|(key, value)| (key.clone(), serde_json::Value::String(value.clone())))
                        .collect(),
                );
                Ok(serde_json::from_value(object)?)
            }
            ConfigurationKind::Json => {
                Ok(serde_json::from_str(self.get_configurations_content()?)?)
            }
            #[cfg(feature = "yaml")]
            ConfigurationKind::Yaml => {
                Ok(serde_yaml::from_str(self.get_configurations_content()?)?)
            }
            #[cfg(feature = "xml")]
            ConfigurationKind::Xml => {
                Ok(serde_xml_rs::from_str(self.get_configurations_content()?)?)
            }
            ConfigurationKind::Txt => {
                let value =
                    serde_json::Value::String(self.get_configurations_content()?.to_string());
                Ok(serde_json::from_value(value)?)
            }
            #[allow(unreachable_patterns)]
            k => panic!(
                "You have to enable feature `{}` for parsing this configuration kind.",
                k
            ),
        }
    }

    /// Deserialize the `configurations` field for `properties`, or `configurations.content` for
    /// other namespace kind, with [`Configuration`] wrapper.
    pub fn deserialize_to_configuration<T: DeserializeOwned>(
        &self,
    ) -> ApolloClientResult<Configuration<T>> {
        self.deserialize_configuration()
            .map(|inner| Configuration::new(inner))
    }
}

type Notifications = Vec<Notification>;

#[derive(Debug, Serialize, Deserialize)]
struct Notification {
    #[serde(rename = "namespaceName")]
    namespace_name: String,
    #[serde(rename = "notificationId")]
    notification_id: i32,
}

fn initialize_notifications(namespace_names: &[&str]) -> Notifications {
    namespace_names
        .iter()
        .map(|namespace_name| Notification {
            namespace_name: namespace_name.to_string(),
            notification_id: -1,
        })
        .collect()
}

/// Represents the apollo client.
pub struct Client<'a> {
    client_config: &'a ClientConfig<'a>,
    notifications: Notifications,
}

impl<'a> Client<'a> {
    /// New with the configuration of apollo and api parameters.
    pub fn with_config(client_config: &'a ClientConfig<'a>) -> Self {
        Self {
            client_config,
            notifications: initialize_notifications(&client_config.namespace_names),
        }
    }

    /// Request apollo config api, and return response of your favorite type.
    pub async fn request<T: FromResponses<Err = ApolloClientError>>(
        &self,
    ) -> ApolloClientResult<T> {
        let mut futures = Vec::with_capacity(self.client_config.namespace_names.len());
        for namespace_name in &self.client_config.namespace_names {
            let url = self.get_config_url(namespace_name, None)?;
            log::debug!("Request apollo config api: {}", &url);
            futures.push(async move { Self::request_response(&url).await });
        }
        let responses = try_join_all(futures).await?;
        log::trace!("Response apollo config data: {:?}", responses);
        FromResponses::from_responses(responses)
    }

    async fn request_response(url: &str) -> ApolloClientResult<Response> {
        let mut response = get_async(url).await?;
        Self::handle_response_status(&response)?;
        let body = response.text_async().await?;
        Ok(serde_json::from_str(&body)?)
    }

    /// Request apollo notification api just once.
    pub async fn listen_once(&mut self) -> ApolloClientResult<()> {
        let client = HttpClientBuilder::new()
            .timeout(DEFAULT_LISTEN_TIMEOUT)
            .build()?;

        let url = self.get_listen_url(&self.notifications)?;
        log::debug!("Request apollo notifications api: {}", &url);
        let mut response = client.get_async(url).await?;
        Self::handle_response_status(&response)?;

        let body = response.text_async().await?;
        let notifications: Notifications = serde_json::from_str(&body)?;
        self.notifications = notifications;
        log::trace!(
            "Response apollo notifications body: {:?}",
            &self.notifications
        );

        Ok(())
    }

    /// Loop and request apollo notification api, if there is a change of the namespaces, return
    /// the response of your favorite type, or [`ApolloClientError`] if there is something wrong.
    pub async fn listen_and_request<T: FromResponses<Err = ApolloClientError>>(
        &mut self,
    ) -> ApolloClientResult<T> {
        loop {
            match self.listen_once().await {
                Ok(()) => return self.request().await,
                Err(ApolloClientError::ApolloNotModified) => {}
                Err(e) => Err(e)?,
            }
        }
    }

    fn handle_response_status<T>(response: &http::Response<T>) -> ApolloClientResult<()> {
        let status = response.status();
        if !status.is_success() {
            match response.status() {
                StatusCode::NOT_MODIFIED => Err(ApolloClientError::ApolloNotModified)?,
                StatusCode::NOT_FOUND => Err(ApolloClientError::ApolloConfigNotFound)?,
                StatusCode::INTERNAL_SERVER_ERROR => Err(ApolloClientError::ApolloServerError)?,
                status => Err(ApolloClientError::ApolloOtherError(status))?,
            }
        }
        Ok(())
    }

    fn get_config_url(
        &self,
        namespace_name: &str,
        release_key: Option<&str>,
    ) -> Result<String, serde_urlencoded::ser::Error> {
        let mut query = Vec::new();
        if let Some(release_key) = release_key {
            query.push(("release_key", release_key));
        }
        if let Some(ip) = &self.client_config.ip {
            query.push(("ip", ip.to_str()));
        }

        let mut query = serde_urlencoded::to_string(query)?;
        if !query.is_empty() {
            query.insert(0, '?');
        }

        Ok(format!(
            "{config_server_url}/configs/{app_id}/{cluster_name}/{namespace_name}{query}",
            config_server_url = self.client_config.config_server_url,
            app_id = self.client_config.app_id,
            cluster_name = self.client_config.cluster_name,
            namespace_name = namespace_name,
            query = query,
        ))
    }

    fn get_listen_url(&self, notifications: &Notifications) -> ApolloClientResult<String> {
        let notifications = if notifications.len() > 0 {
            #[derive(Serialize)]
            struct NotificationsQuery {
                notifications: String,
            }
            let notifications = NotificationsQuery {
                notifications: serde_json::to_string(notifications.deref())?,
            };
            let mut notifications = serde_urlencoded::to_string(notifications)?;
            notifications.insert(0, '&');
            notifications
        } else {
            "".to_string()
        };

        Ok(format!(
            "{config_server_url}/notifications/v2?appId={app_id}&cluster={cluster_name}{notifications}",
            config_server_url = self.client_config.config_server_url,
            app_id = self.client_config.app_id,
            cluster_name = self.client_config.cluster_name,
            notifications = notifications,
        ))
    }
}