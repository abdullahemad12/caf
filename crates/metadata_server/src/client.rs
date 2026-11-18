use std::time;

use observability::error::{CafError, WrapErrorInResult};
use reqwest::{Client, Url};
use serde::{Serialize, de::DeserializeOwned};

use crate::api::{
    EndpointDefinition, GET_METADATA_ENDPOINT, GetPackageMetadataResponse,
    GetPackageMetadataUrlParams, HttpMethod,
};

pub struct MetadataClient {
    base_url: Url,
    client: Client,
}

#[derive(Clone)]
pub struct MetadataClientConfig {
    server_host: String,
    server_port: u16,
    timeout: time::Duration,
}

impl MetadataClientConfig {
    pub fn new(server_host: impl Into<String>, server_port: u16) -> Self {
        MetadataClientConfig {
            server_host: server_host.into(),
            server_port,
            timeout: std::time::Duration::from_secs(10),
        }
    }

    fn with_custom_configs<F>(&self, apply: F) -> Self
    where
        F: FnOnce(&mut Self),
    {
        let mut conf = self.clone();
        apply(&mut conf);
        conf
    }

    pub fn timeout(&self, timeout: time::Duration) -> Self {
        self.with_custom_configs(|conf| conf.timeout = timeout)
    }
}

pub struct GetPackageMetadataParams {
    name: String,
    version: String,
    arch: String,
    os: String,
}

impl MetadataClient {
    pub fn new(conf: MetadataClientConfig) -> Result<Self, CafError> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            // allow insecure connection in development mode only
            .https_only(!cfg!(debug_assertions))
            .build()
            .wrap_err("unable to initialize the http client")?;

        let base_url = Url::parse(
            format!(
                "{}:{}",
                conf.server_host
                    .trim_end_matches(|c| ['/', ':'].contains(&c)),
                conf.server_port
            )
            .as_str(),
        )
        .wrap_err("the url for the metadata server is malformed")?;

        Ok(MetadataClient { base_url, client })
    }

    pub async fn get_package_metadata(
        &self,
        params: GetPackageMetadataParams,
    ) -> Result<GetPackageMetadataResponse, CafError> {
        let params = GetPackageMetadataUrlParams {
            name: params.name,
            version: params.version,
            arch: params.arch,
            os: params.os,
        };

        self.request(GET_METADATA_ENDPOINT, params, (), ()).await
    }

    async fn request<TUrlParams, TQueryParams, TRequestBody, TResponse>(
        &self,
        endpoint: EndpointDefinition<TUrlParams, TQueryParams, TRequestBody, TResponse>,
        url_params: TUrlParams,
        query_params: TQueryParams,
        request_body: TRequestBody,
    ) -> Result<TResponse, CafError>
    where
        TResponse: DeserializeOwned,
        TQueryParams: Serialize,
        TRequestBody: Serialize,
    {
        let url = self
            .base_url
            .join(endpoint.path(url_params).as_str())
            .wrap_err(format!(
                "unable to construct the URL for {}",
                endpoint.resource()
            ))?;

        let req = self
            .client
            .request(endpoint.http_method().into(), url)
            .query(&query_params);

        let req = if endpoint.http_method() == HttpMethod::Get {
            req
        } else {
            req.json(&request_body)
        };

        // TODO: map Http errors
        let resp = req
            .send()
            .await
            .wrap_err(format!("making request to {} failed", endpoint.resource()))?;

        resp.json()
            .await
            .wrap_err("unable to deserialize the response")
    }
}
