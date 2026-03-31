//! Magic link authentication routes

use crate::error::CloudError;
use crate::state::CloudState;
use actix_web::{web, HttpResponse};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct SendLinkRequest {
    pub email: String,
}

#[derive(Deserialize)]
pub struct VerifyQuery {
    pub token: String,
}

/// POST /api/auth/send — send a magic link to the customer's email
pub async fn send_magic_link(
    state: web::Data<CloudState>,
    body: web::Json<SendLinkRequest>,
) -> HttpResponse {
    let email = body.email.trim().to_lowercase();

    // Rate limit: one magic link per email per 60 seconds
    if let Ok(true) = crate::db::has_recent_magic_link(&state.db, &email, 60).await {
        return HttpResponse::Ok().json(serde_json::json!({
            "status": "sent",
            "message": "If an account exists, a sign-in link has been sent"
        }));
    }

    // Always return success to avoid leaking whether an email exists
    let customer = match crate::db::get_customer_by_email(&state.db, &email).await {
        Ok(Some(c)) => c,
        _ => {
            return HttpResponse::Ok().json(serde_json::json!({
                "status": "sent",
                "message": "If an account exists, a sign-in link has been sent"
            }));
        }
    };

    // Verify they have an active instance
    match crate::db::get_instance_by_customer_id(&state.db, customer.id).await {
        Ok(Some(_)) => {}
        _ => {
            return HttpResponse::Ok().json(serde_json::json!({
                "status": "sent",
                "message": "If an account exists, a sign-in link has been sent"
            }));
        }
    }

    // Create magic link token (expires in 15 minutes)
    let token = Uuid::new_v4().to_string();
    let expires_at = chrono::Utc::now() + chrono::Duration::minutes(15);

    if let Err(e) = crate::db::create_magic_link(&state.db, &email, &token, expires_at).await {
        eprintln!("Failed to create magic link: {e}");
        return CloudError::Internal("Failed to create sign-in link".into()).to_response();
    }

    // Send email
    let link = format!("{}/auth/verify?token={}", state.config.public_url, token);
    if let Err(e) = state.mailgun.send_magic_link(&email, &link).await {
        eprintln!("Failed to send magic link email: {e}");
        return CloudError::Internal("Failed to send email".into()).to_response();
    }

    HttpResponse::Ok().json(serde_json::json!({
        "status": "sent",
        "message": "If an account exists, a sign-in link has been sent"
    }))
}

/// GET /api/auth/verify — verify a magic link token and return management token
pub async fn verify_magic_link(
    state: web::Data<CloudState>,
    query: web::Query<VerifyQuery>,
) -> HttpResponse {
    let email = match crate::db::consume_magic_link(&state.db, &query.token).await {
        Ok(Some(email)) => email,
        Ok(None) => {
            return CloudError::Unauthorized("Invalid or expired link".into()).to_response()
        }
        Err(e) => return e.to_response(),
    };

    // Look up customer and instance
    let customer = match crate::db::get_customer_by_email(&state.db, &email).await {
        Ok(Some(c)) => c,
        _ => return CloudError::NotFound("Account not found".into()).to_response(),
    };

    match crate::db::get_instance_by_customer_id(&state.db, customer.id).await {
        Ok(Some(instance)) => HttpResponse::Ok().json(serde_json::json!({
            "management_token": instance.management_token,
            "instance_id": instance.id,
            "status": instance.status,
        })),
        _ => CloudError::NotFound("No active instance found".into()).to_response(),
    }
}
