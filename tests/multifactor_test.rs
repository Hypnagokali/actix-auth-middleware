use std::{error::Error as StdError, future::ready, net::SocketAddr, sync::Arc, thread};

use actix_session::{storage::CookieSessionStore, SessionMiddleware};
use actix_web::{cookie::Key, get, post, web::{self, Path}, App, HttpRequest, HttpResponse, HttpServer, Responder};
use auth_middleware_for_actix_web::{
    google_auth::google_auth::GoogleAuth, middleware::{AuthMiddleware, PathMatcher}, multifactor::{OptionalFactor, TotpSecretRepository}, session::session_auth::{SessionAuthProvider, UserSession}, web::{ErrorResponse, MfaRequestBody}, AuthToken
};
use reqwest::{Client, StatusCode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;

#[derive(Serialize, Deserialize)]
pub struct User {
    pub email: String,
    pub name: String,
}

struct TotpTestRepo;

#[derive(Error, Debug)]
#[error("No secret found in repo")]
struct NoSecretFoundError;

impl<U> TotpSecretRepository<U> for TotpTestRepo 
where 
    U: DeserializeOwned
{
    type Error = NoSecretFoundError;

    fn get_auth_secret(&self, _user: &U) -> impl std::future::Future<Output = Result<String, Self::Error>> {
        Box::pin(ready(Ok("I3VFM3JKMNDJCDH5BMBEEQAW6KJ6NOE3".to_owned())))
    }
}


// ToDo: should be created by a macro or automatically if possible
#[post("/login/mfa")]
pub async fn mfa_route(factor: OptionalFactor, body: web::Json<MfaRequestBody>, req: HttpRequest, token: AuthToken<User>) -> impl Responder {
    if let Some(f) = factor.get_value() {
        match f.check_code(body.get_code(), &req).await {
            Ok(_) => {
                token.mfa_challenge_done();
                return HttpResponse::Ok().finish();
            },
            Err(e) => HttpResponse::BadRequest().json(ErrorResponse::from(e))
        }
   } else {
    HttpResponse::BadRequest().finish()
   }
}

#[get("/secured-route")]
pub async fn secured_route(token: AuthToken<User>) -> impl Responder {
    HttpResponse::Ok().body(format!(
        "Request from user: {}",
        token.get_authenticated_user().email
    ))
}

#[post("/logout")]
pub async fn logout(token: AuthToken<User>) -> impl Responder {
    token.invalidate();
    HttpResponse::Ok()
}

#[post("/login")]
async fn login(session: UserSession) -> impl Responder {
    // For session based authentication we need to manually check user and password and save the user in the session
    let user = User {
        email: "jenny@example.org".to_owned(),
        name: "Jenny B.".to_owned(),
    };

    session
        .set_user(user)
        .expect("User could not be set in session");
    return HttpResponse::Ok();
}

fn create_actix_session_middleware() -> SessionMiddleware<CookieSessionStore> {
    let key = Key::generate();

    SessionMiddleware::new(CookieSessionStore::default(), key.clone())
}

// TODO
// - call mfa route without calling login before
// - mfa success
// - mfa failure 

#[actix_rt::test]
async fn should_not_be_looged_in_without_mfa() {
    let addr = actix_test::unused_addr();
    start_test_server(addr);

    let client = Client::builder().cookie_store(true).build().unwrap();

    client
        .post(format!("http://{addr}/login"))
        .send()
        .await
        .unwrap();

    let res = client
        .get(format!("http://{addr}/secured-route"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

fn start_test_server(addr: SocketAddr) {
    thread::spawn(move || {
        actix_rt::System::new()
            .block_on(async {
                let totp_secret_repo = Arc::new(TotpTestRepo);

                HttpServer::new(move || {
                    App::new()
                        .service(secured_route)
                        .service(login)
                        .service(logout)
                        .service(mfa_route)
                        .wrap(AuthMiddleware::<_, User>::new_with_factor(
                            SessionAuthProvider,
                            PathMatcher::default(),
                            Box::new(GoogleAuth::<_, User>::new(Arc::clone(&totp_secret_repo)))
                        ))
                        .wrap(create_actix_session_middleware())
                })
                .bind(format!("{addr}"))
                .unwrap()
                .run()
                .await
            })
            .unwrap();
    });
}