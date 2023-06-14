use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
};

use axum::{routing, Router, Server};

use hyper::Error;
use utoipa::{
    openapi::security::{ApiKey, ApiKeyValue, SecurityScheme},
    Modify, OpenApi,
};
use utoipa_swagger_ui::SwaggerUi;

use crate::smap::Store;

use axum::extract::DefaultBodyLimit;

#[tokio::main]
async fn main() -> Result<(), Error> {
    #[derive(OpenApi)]
    #[openapi(
        paths(
            smap::list_smaps,
            smap::upload_smap_multipart,
        ),
        components(
            schemas(smap::SMap, smap::SMapError, smap::NewSMap)
        ),
        modifiers(&SecurityAddon),
        tags(
            (name = "static map", description = "Static Map items management API")
        )
    )]
    struct ApiDoc;

    struct SecurityAddon;

    impl Modify for SecurityAddon {
        fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
            if let Some(components) = openapi.components.as_mut() {
                components.add_security_scheme(
                    "api_key",
                    SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("smap_apikey"))),
                )
            }
        }
    }

    let store = Arc::new(Store::default());
    let app = Router::new()
        .merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .route("/smap", routing::get(smap::list_smaps))
        .route("/upload", routing::post(smap::upload_smap_multipart))
        .layer(DefaultBodyLimit::disable())
        .layer(DefaultBodyLimit::max(1024))
        .with_state(store);

    let address = SocketAddr::from((Ipv4Addr::UNSPECIFIED, 8080));
    Server::bind(&address).serve(app.into_make_service()).await
}

mod smap {
    use axum::{
        extract::{Multipart, Path, Query, State},
        response::IntoResponse,
        Json,
    };
    use hyper::{HeaderMap, StatusCode};
    use serde::{Deserialize, Serialize};
    use std::sync::Arc;
    use tokio::fs::File;
    use tokio::io::AsyncWriteExt;
    use tokio::sync::Mutex;
    use utoipa::{IntoParams, ToSchema};
    use uuid::Uuid;

    use utoipa::openapi::schema::KnownFormat;

    /// In-memory static map store.
    pub(super) type Store = Mutex<Vec<SMap>>;

    #[derive(ToSchema)]
    pub(super) struct NewSMap {
        #[schema(example = "Tropical Cyclone exposed population")]
        title: String,
        file: Vec<u8>,
    }

    /// Item to do.
    #[derive(Serialize, Deserialize, ToSchema, Clone, Debug)]
    pub(super) struct SMap {
        uuid: String,
        #[schema(example = "Tropical Cyclone exposed population")]
        title: String,
        path: String,
    }

    impl SMap {
        fn new(uuid: String, title: String, path: String) -> Self {
            Self { uuid, title, path }
        }
    }

    /// Static maps operation errors
    #[derive(Serialize, Deserialize, ToSchema)]
    pub(super) enum SMapError {
        /// SMap already exists conflict.
        #[schema(example = "Static map already exists")]
        Conflict(String),
        /// SMap not found by id.
        #[schema(example = "uuid = dsadkasdasdasd")]
        NotFound(String),
        /// SMap operation unauthorized
        #[schema(example = "missing api key")]
        Unauthorized(String),
    }

    /// List all Smap items
    ///
    /// List all Smap items from in-memory storage.
    #[utoipa::path(
        get,
        path = "/smap",
        responses(
            (status = 200, description = "List all static maps successfully", body = [SMap])
        )
    )]
    pub(super) async fn list_smaps(State(store): State<Arc<Store>>) -> Json<Vec<SMap>> {
        let smaps = store.lock().await.clone();
        Json(smaps)
    }

    /// Uppload Static map
    ///
    /// Tries to upload a new SMap item to in-memory storage or fails with 409 conflict if already exists.
    #[utoipa::path(
        post,
        path = "/upload",
        request_body(content=NewSMap, content_type = "multipart/form-data")
    )]
    pub(super) async fn upload_smap_multipart(mut multipart: Multipart) -> impl IntoResponse {
        let mut title: Option<String> = None;
        let mut path: Option<String> = None;

        let uuid = Uuid::new_v4().to_string();

        while let Some(field) = multipart.next_field().await.unwrap() {
            let name = field.name().unwrap().to_string();

            if name == "title" {
                title = Some(field.text().await.unwrap());
                continue;
            }
            let file_name = field.file_name().unwrap().to_owned();

            let bytes = field.bytes().await.unwrap();

            let file_path = format!("/tmp/{file_name}");
            let mut file = File::create(&file_path).await.unwrap();

            file.write_all(&bytes).await.unwrap();

            path = Some(file_path);
            //println!("Length of `{}` is {} bytes", name, data.len());
        }

        let smap = SMap::new(uuid, title.unwrap(), path.unwrap());
        println!("{:?}", smap);

        (StatusCode::CREATED, Json(smap)).into_response()
    }
}
