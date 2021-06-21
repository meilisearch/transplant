use actix_web::{delete, get, post, web, HttpResponse};

use crate::helpers::Authentication;
use crate::index::Settings;
use crate::Data;
use crate::{error::ResponseError, index::Unchecked};

#[macro_export]
macro_rules! make_setting_route {
    ($route:literal, $type:ty, $attr:ident) => {
        mod $attr {
            use actix_web::{web, HttpResponse};

            use crate::data;
            use crate::error::ResponseError;
            use crate::helpers::Authentication;
            use crate::index::Settings;

            #[actix_web::delete($route, wrap = "Authentication::Private")]
            pub async fn delete(
                data: web::Data<data::Data>,
                index_uid: web::Path<String>,
            ) -> Result<HttpResponse, ResponseError> {
                use crate::index::Settings;
                let settings = Settings {
                    $attr: Some(None),
                    ..Default::default()
                };
                let update_status = data.update_settings(index_uid.into_inner(), settings, false).await?;
                Ok(HttpResponse::Accepted().json(serde_json::json!({ "updateId": update_status.id() })))
            }

            #[actix_web::post($route, wrap = "Authentication::Private")]
            pub async fn update(
                data: actix_web::web::Data<data::Data>,
                index_uid: actix_web::web::Path<String>,
                body: actix_web::web::Json<Option<$type>>,
            ) -> std::result::Result<HttpResponse, ResponseError> {
                let settings = Settings {
                    $attr: Some(body.into_inner()),
                    ..Default::default()
                };

                let update_status = data.update_settings(index_uid.into_inner(), settings, true).await?;
                Ok(HttpResponse::Accepted().json(serde_json::json!({ "updateId": update_status.id() })))
            }

            #[actix_web::get($route, wrap = "Authentication::Private")]
            pub async fn get(
                data: actix_web::web::Data<data::Data>,
                index_uid: actix_web::web::Path<String>,
            ) -> std::result::Result<HttpResponse, ResponseError> {
                let settings = data.settings(index_uid.into_inner()).await?;
                Ok(HttpResponse::Ok().json(settings.$attr))
            }
        }
    };
}

make_setting_route!(
    "/indexes/{index_uid}/settings/filterable-attributes",
    std::collections::HashSet<String>,
    filterable_attributes
);

make_setting_route!(
    "/indexes/{index_uid}/settings/displayed-attributes",
    Vec<String>,
    displayed_attributes
);

make_setting_route!(
    "/indexes/{index_uid}/settings/searchable-attributes",
    Vec<String>,
    searchable_attributes
);

make_setting_route!(
    "/indexes/{index_uid}/settings/stop-words",
    std::collections::BTreeSet<String>,
    stop_words
);

make_setting_route!(
    "/indexes/{index_uid}/settings/synonyms",
    std::collections::BTreeMap<String, Vec<String>>,
    synonyms
);

make_setting_route!(
    "/indexes/{index_uid}/settings/distinct-attribute",
    String,
    distinct_attribute
);

make_setting_route!(
    "/indexes/{index_uid}/settings/ranking-rules",
    Vec<String>,
    ranking_rules
);

macro_rules! create_services {
    ($($mod:ident),*) => {
        pub fn services(cfg: &mut web::ServiceConfig) {
            cfg
                .service(update_all)
                .service(get_all)
                .service(delete_all)
                $(
                    .service($mod::get)
                    .service($mod::update)
                    .service($mod::delete)
                )*;
        }
    };
}

create_services!(
    filterable_attributes,
    displayed_attributes,
    searchable_attributes,
    distinct_attribute,
    stop_words,
    synonyms,
    ranking_rules
);

#[post("/indexes/{index_uid}/settings", wrap = "Authentication::Private")]
async fn update_all(
    data: web::Data<Data>,
    index_uid: web::Path<String>,
    body: web::Json<Settings<Unchecked>>,
) -> Result<HttpResponse, ResponseError> {
    let settings = body.into_inner().check();
    let update_result = data
        .update_settings(index_uid.into_inner(), settings, true)
        .await?;
    let json = serde_json::json!({ "updateId": update_result.id() });
    Ok(HttpResponse::Accepted().json(json))
}

#[get("/indexes/{index_uid}/settings", wrap = "Authentication::Private")]
async fn get_all(
    data: web::Data<Data>,
    index_uid: web::Path<String>,
) -> Result<HttpResponse, ResponseError> {
    let settings = data.settings(index_uid.into_inner()).await?;
    Ok(HttpResponse::Ok().json(settings))
}

#[delete("/indexes/{index_uid}/settings", wrap = "Authentication::Private")]
async fn delete_all(
    data: web::Data<Data>,
    index_uid: web::Path<String>,
) -> Result<HttpResponse, ResponseError> {
    let settings = Settings::cleared();
    let update_result = data
        .update_settings(index_uid.into_inner(), settings, false)
        .await?;
    let json = serde_json::json!({ "updateId": update_result.id() });
    Ok(HttpResponse::Accepted().json(json))
}