use actix_web::{HttpResponse, Responder};
use askama::Template;

#[derive(Template)]
#[template(path = "chat_youtube.html")]
struct ChatYoutubeTemplate;

pub async fn chat_youtube() -> impl Responder {
    let tmpl = ChatYoutubeTemplate;
    let rendered = tmpl.render().unwrap();
    HttpResponse::Ok().content_type("text/html").body(rendered)
}

