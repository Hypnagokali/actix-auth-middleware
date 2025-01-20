use std::{future::{ready, Future}, marker::PhantomData, pin::Pin, sync::Arc};

use actix_web::{HttpMessage, HttpRequest};
use google_authenticator::GoogleAuthenticator;
use serde::de::DeserializeOwned;

use crate::{multifactor::{CheckCodeError, ConditionCheckError, Factor, GenerateCodeError, TotpSecretRepository}, AuthToken};


pub struct GoogleAuth<T, U> 
where
    T: TotpSecretRepository<U>,
    U: DeserializeOwned
{
    totp_secret_repo: Arc<T>,
    phantom_data_user: PhantomData<U>,
}

impl<T, U> GoogleAuth<T, U>
where
    T: TotpSecretRepository<U>,
    U: DeserializeOwned {
        pub fn new(totp_secret_repo: Arc<T>) -> Self {
            Self {
                totp_secret_repo: Arc::clone(&totp_secret_repo),
                phantom_data_user: PhantomData,
            }
        }
}

impl<T, U> Factor for GoogleAuth<T, U> 
where
    T: TotpSecretRepository<U> +'static,
    U: DeserializeOwned + 'static,
{
    fn generate_code(&self, _req: &actix_web::HttpRequest) -> Result<Option<String>, GenerateCodeError> {
        Ok(None)   
    }

    fn check_code(&self, code: &str, req: &HttpRequest) -> Pin<Box<dyn Future<Output = Result<(), CheckCodeError>>>> {
        let extensions = req.extensions();
        let token = match extensions.get::<AuthToken<U>>() {
            Some(token) => {
                token
            },
            None => return Box::pin(ready(Err(CheckCodeError::UnknownError("Cant load AuthToken".to_owned()))))
        };

        let token_to_check = AuthToken::from_ref(token);
        let repo = Arc::clone(&self.totp_secret_repo);
        let code_to_check = code.to_owned();
        Box::pin(async move {
            let u = &token_to_check.get_authenticated_user();
            repo.get_auth_secret(u).await
            .map(|secret| {
                // ToDo: check if constructing with new is okay
                let authenticator = GoogleAuthenticator::new();
                if authenticator.verify_code(&secret, &code_to_check, 0, 0) {
                    Ok(())
                } else {
                    Err(CheckCodeError::InvalidCode)
                }
                    
            })
            .unwrap_or_else(|_| // ToDo: use error type 
                Err(CheckCodeError::UnknownError("Something went wrong".to_owned()))
            )
        })
    }
    
    fn is_condition_met(&self, _: &HttpRequest) -> Pin<Box<dyn Future<Output = Result<bool, ConditionCheckError>>>> {
        Box::pin(async {
            Ok(true)
        })
    }
    
    fn get_unique_id(&self) -> String {
        "GAUTH".to_owned()
    }
}