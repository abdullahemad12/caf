use crate::api::{GET_METADATA_ENDPOINT, GetPackageMetadataResponse, GetPackageMetadataUrlParams};
use axum::{Json, Router, extract::Path, http::StatusCode, response::IntoResponse};

async fn get_package_metadata(
    Path(GetPackageMetadataUrlParams {
        name,
        version,
        arch,
        os,
    }): Path<GetPackageMetadataUrlParams>,
) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(GetPackageMetadataResponse {
            name,
            version,
            arch,
            os,
            hash: "".to_string(),
        }),
    )
}

pub fn create_app() -> Router {
    Router::new().route(
        GET_METADATA_ENDPOINT.resource(),
        GET_METADATA_ENDPOINT.serve(get_package_metadata),
    )
}
