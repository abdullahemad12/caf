use std::marker::PhantomData;

use axum::{
    handler::Handler,
    http,
    routing::{MethodRouter, get},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

///////////////////////////////////
// Endpoints
///////////////////////////////////
pub(crate) const GET_METADATA_ENDPOINT: EndpointDefinition<
    GetPackageMetadataUrlParams,
    (),
    (),
    GetPackageMetadataResponse,
> = EndpointDefinition::<GetPackageMetadataUrlParams, (), (), GetPackageMetadataResponse>::new(
    "/package/{package_name}/version/{version}/arch/{arch}/os/{os}/metadata",
    HttpMethod::Get,
    |GetPackageMetadataUrlParams {
         name,
         version,
         arch,
         os,
     }| { format!("/package/{name}/version/{version}/arch/{arch}/os/{os}/metadata") },
);

////////////////////////////////
// Messages Structs
///////////////////////////////
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetPackageMetadataResponse {
    pub name: String,
    pub version: String,
    pub arch: String,
    pub os: String,
    pub hash: String,
}

#[derive(Deserialize)]
pub(crate) struct GetPackageMetadataUrlParams {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) arch: String,
    pub(crate) os: String,
}

///////////////////////////////
/// Endpoint definition structs
///////////////////////////////
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub(crate) enum HttpMethod {
    Get,
}

impl From<HttpMethod> for http::Method {
    fn from(method: HttpMethod) -> http::Method {
        match method {
            HttpMethod::Get => http::Method::GET,
        }
    }
}

pub(crate) struct EndpointDefinition<TUrlParams, TQueryParams, TRequestBody, TResponse>
where
    TResponse: DeserializeOwned,
    TQueryParams: Serialize,
    TRequestBody: Serialize,
{
    resource: &'static str,
    http_method: HttpMethod,
    path: fn(TUrlParams) -> String,
    _url_params_marker: PhantomData<TUrlParams>,
    _query_params_marker: PhantomData<TQueryParams>,
    _req_marker: PhantomData<TRequestBody>,
    _resp_marker: PhantomData<TResponse>,
}

impl<TUrlParams, TQueryParams, TRequestBody, TResponse>
    EndpointDefinition<TUrlParams, TQueryParams, TRequestBody, TResponse>
where
    TResponse: DeserializeOwned,
    TQueryParams: Serialize,
    TRequestBody: Serialize,
{
    const fn new(
        resource: &'static str,
        http_method: HttpMethod,
        path: fn(TUrlParams) -> String,
    ) -> Self {
        Self {
            resource,
            http_method,
            path,
            _url_params_marker: PhantomData,
            _query_params_marker: PhantomData,
            _req_marker: PhantomData,
            _resp_marker: PhantomData,
        }
    }

    pub(crate) fn serve<F, T, S>(&self, f: F) -> MethodRouter<S>
    where
        S: Sync + Clone + Send + 'static,
        T: 'static,
        F: Handler<T, S>,
    {
        match self.http_method {
            HttpMethod::Get => get(f),
        }
    }

    pub(crate) fn http_method(&self) -> HttpMethod {
        self.http_method
    }

    pub(crate) fn resource(&self) -> &'static str {
        self.resource
    }

    pub(crate) fn path(&self, url_params: TUrlParams) -> String {
        (self.path)(url_params)
    }
}
