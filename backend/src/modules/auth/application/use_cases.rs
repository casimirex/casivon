use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use chrono::{DateTime, Duration, Utc};
// `OsRng` is already in scope from argon2's re-export; both crates sit on the
// same rand_core version, so it satisfies this trait too.
use rand::RngCore;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::config::AppConfig;
use crate::error::{AppError, AppResult};
use crate::modules::auth::application::dto::*;
use crate::modules::auth::domain::entities::{
    EmailVerificationToken, PasswordResetToken, User, UserProfile,
};
use crate::modules::auth::domain::repositories::{
    EmailVerificationTokenRepository, PasswordResetTokenRepository, RevokedTokenStore,
    UserRepository,
};
use crate::shared::auth::{
    create_access_token, create_refresh_token, verify_token_of_type, TOKEN_TYPE_REFRESH,
};
use crate::shared::email::{EmailMessage, EmailSender};

/// How long a reset link stays usable. Long enough to survive a slow mail
/// server and a distracted user, short enough that a link sitting in an
/// unattended inbox stops being a way in.
const RESET_TOKEN_TTL_MINUTES: i64 = 60;

/// Minimum gap between reset emails for one account, so the endpoint cannot be
/// used to flood someone's inbox.
const RESET_REQUEST_INTERVAL_SECONDS: i64 = 60;

/// Returned whatever happens, so the endpoint cannot be used to find out which
/// addresses have accounts.
const RESET_REQUESTED_MESSAGE: &str =
    "If that address has an account, a reset link is on its way.";

/// Returned for a token that is unknown, expired, already spent, or belongs to
/// a deleted user — telling the four apart would only help someone guessing.
const INVALID_RESET_TOKEN: &str = "This reset link is invalid or has expired";

/// How long a verification link stays usable. Far longer than a reset link:
/// there is nothing sensitive behind it — the worst a stolen one does is
/// confirm an address its holder already controls — and a new signup who
/// verifies a day later should not have to ask for another.
const VERIFICATION_TOKEN_TTL_HOURS: i64 = 48;

/// Minimum gap between verification emails for one account.
const VERIFICATION_REQUEST_INTERVAL_SECONDS: i64 = 60;

/// Returned whatever happens, for the same reason the reset endpoint does it.
const VERIFICATION_REQUESTED_MESSAGE: &str =
    "If that address has an unverified account, a new link is on its way.";

const INVALID_VERIFICATION_TOKEN: &str = "This verification link is invalid or has expired";

pub struct AuthUseCases<
    R: UserRepository,
    T: PasswordResetTokenRepository,
    V: EmailVerificationTokenRepository,
> {
    repo: R,
    reset_tokens: T,
    verification_tokens: V,
    config: AppConfig,
    revoked_tokens: std::sync::Arc<dyn RevokedTokenStore>,
    email: std::sync::Arc<dyn EmailSender>,
}

impl<R: UserRepository, T: PasswordResetTokenRepository, V: EmailVerificationTokenRepository>
    AuthUseCases<R, T, V>
{
    pub fn new(
        repo: R,
        reset_tokens: T,
        verification_tokens: V,
        config: AppConfig,
        revoked_tokens: std::sync::Arc<dyn RevokedTokenStore>,
        email: std::sync::Arc<dyn EmailSender>,
    ) -> Self {
        Self { repo, reset_tokens, verification_tokens, config, revoked_tokens, email }
    }

    pub async fn register(&self, req: RegisterRequest) -> AppResult<AuthResponse> {
        if self.repo.find_by_email(&req.email).await?.is_some() {
            return Err(AppError::Conflict("User already exists".to_string()));
        }

        let password_hash = hash_password(&req.password)?;
        let now = Utc::now();

        // Bootstrap: the first account on a fresh install owns the instance.
        // Without this nobody could ever reach the role-gated modules, since
        // granting a role itself needs an admin.
        let role = if self.repo.count().await? == 0 { "admin" } else { "user" };

        let user = User {
            id: Uuid::new_v4(),
            email: req.email,
            password_hash,
            first_name: req.first_name,
            last_name: req.last_name,
            role: role.to_string(),
            org_id: None,
            is_active: true,
            email_verified: false,
            // Ignored on insert — the column defaults to 0. Carried on the
            // struct because `RETURNING *` reads it straight back.
            session_epoch: 0,
            created_at: now,
            updated_at: now,
        };

        let user = self.repo.create(&user).await?;

        // Sent, but not waited on and not gating anything: registration succeeds
        // whether or not the mail goes out. Failing a signup because a relay was
        // briefly down would lose the account for a step that can be repeated
        // from the prompt at any time.
        self.send_verification_link(&user).await;

        self.create_auth_response(user).await
    }

    /// Issues a verification link and mails it.
    ///
    /// Errors are logged rather than returned: every caller either must not fail
    /// for this (registration) or must not reveal whether the address exists
    /// (resend).
    async fn send_verification_link(&self, user: &User) {
        if let Err(error) = self.issue_verification_token(user).await {
            tracing::error!("failed to send a verification email: {error}");
        }
    }

    async fn issue_verification_token(&self, user: &User) -> AppResult<()> {
        let now = Utc::now();
        let token = generate_email_token();

        self.verification_tokens
            .create(&EmailVerificationToken {
                id: Uuid::new_v4(),
                user_id: user.id,
                token_hash: hash_email_token(&token),
                expires_at: now + Duration::hours(VERIFICATION_TOKEN_TTL_HOURS),
                used_at: None,
                created_at: now,
            })
            .await?;

        let link = format!(
            "{}/verify-email?token={}",
            self.config.app_base_url.trim_end_matches('/'),
            token
        );

        self.email
            .send(EmailMessage {
                to: user.email.clone(),
                subject: "Confirm your email address".to_string(),
                body: format!(
                    "Hello {},\n\n\
                     Use the link below to confirm this address belongs to you. \
                     It works once and expires in {} hours.\n\n\
                     {}\n\n\
                     If you did not create an account, you can ignore this email.",
                    user.first_name, VERIFICATION_TOKEN_TTL_HOURS, link
                ),
            })
            .await
    }

    /// Confirms an address from the link in the email.
    pub async fn verify_email(&self, req: VerifyEmailRequest) -> AppResult<VerifyEmailResponse> {
        let now = Utc::now();
        let invalid = || AppError::Auth(INVALID_VERIFICATION_TOKEN.to_string());

        let token = self
            .verification_tokens
            .find_by_hash(&hash_email_token(&req.token))
            .await?
            .ok_or_else(invalid)?;

        if !token.is_usable(now) {
            return Err(invalid());
        }

        // Spend before recording, so two clicks of the same link resolve to one
        // winner — the same ordering the password reset uses.
        if !self.verification_tokens.mark_used(token.id, now).await? {
            return Err(invalid());
        }

        let user = self.repo.find_by_id(token.user_id).await?.ok_or_else(invalid)?;
        self.repo.mark_email_verified(user.id).await?;

        // Any other links already in the inbox are now stale.
        self.verification_tokens.expire_all_for_user(user.id, now).await?;

        Ok(VerifyEmailResponse { email: user.email, email_verified: true })
    }

    /// Sends another verification link.
    ///
    /// Answers the same way whatever happens, for the reason `forgot_password`
    /// does: an endpoint that said "no such account" or "already verified" would
    /// be a way to probe which addresses are registered here.
    pub async fn resend_verification(
        &self,
        req: ResendVerificationRequest,
    ) -> AppResult<ForgotPasswordResponse> {
        let acknowledge =
            || Ok(ForgotPasswordResponse { message: VERIFICATION_REQUESTED_MESSAGE.to_string() });

        let Some(user) = self.repo.find_by_email(&req.email).await? else {
            return acknowledge();
        };
        if !user.is_active || user.email_verified {
            return acknowledge();
        }

        let now = Utc::now();
        if let Some(last_issued) = self.verification_tokens.last_issued_at(user.id).await? {
            if now - last_issued < Duration::seconds(VERIFICATION_REQUEST_INTERVAL_SECONDS) {
                return acknowledge();
            }
        }

        self.send_verification_link(&user).await;
        acknowledge()
    }

    pub async fn login(&self, req: LoginRequest) -> AppResult<AuthResponse> {
        let user = self.repo.find_by_email(&req.email).await?
            .ok_or_else(|| AppError::Auth("Invalid credentials".to_string()))?;

        if !user.is_active {
            return Err(AppError::Auth("Account is disabled".to_string()));
        }

        verify_password(&req.password, &user.password_hash)?;
        self.create_auth_response(user).await
    }

    /// Exchanges a valid refresh token for a fresh pair. The user is re-read from
    /// the database so a deactivated account cannot keep refreshing its way in.
    pub async fn refresh(&self, req: RefreshTokenRequest) -> AppResult<AuthResponse> {
        let claims =
            verify_token_of_type(&req.refresh_token, &self.config.jwt_secret, TOKEN_TYPE_REFRESH)?;

        // Signing out is only meaningful if the token stops being accepted here.
        if self.revoked_tokens.is_revoked(claims.jti).await? {
            return Err(AppError::Auth("This session has been signed out".to_string()));
        }

        let user = self
            .repo
            .find_by_id(claims.sub)
            .await?
            .ok_or_else(|| AppError::Auth("Invalid refresh token".to_string()))?;

        if !user.is_active {
            return Err(AppError::Auth("Account is disabled".to_string()));
        }

        // Changing the password bumps the epoch, which is what ends sessions the
        // user cannot reach to sign out of — an old laptop, or whoever prompted
        // the reset in the first place.
        if claims.epoch != user.session_epoch {
            return Err(AppError::Auth(
                "This session ended when the password was changed".to_string(),
            ));
        }

        self.create_auth_response(user).await
    }

    /// Starts a password reset.
    ///
    /// Always reports the same thing. An endpoint that answered "no such
    /// account" would be a way to test whether an address is registered here,
    /// which is worth more to an attacker than it is to the person typing.
    pub async fn forgot_password(
        &self,
        req: ForgotPasswordRequest,
    ) -> AppResult<ForgotPasswordResponse> {
        let acknowledge =
            || Ok(ForgotPasswordResponse { message: RESET_REQUESTED_MESSAGE.to_string() });

        let Some(user) = self.repo.find_by_email(&req.email).await? else {
            return acknowledge();
        };
        if !user.is_active {
            return acknowledge();
        }

        let now = Utc::now();
        if let Some(last_issued) = self.reset_tokens.last_issued_at(user.id).await? {
            if now - last_issued < Duration::seconds(RESET_REQUEST_INTERVAL_SECONDS) {
                return acknowledge();
            }
        }

        let token = generate_email_token();
        self.reset_tokens
            .create(&PasswordResetToken {
                id: Uuid::new_v4(),
                user_id: user.id,
                token_hash: hash_email_token(&token),
                expires_at: now + Duration::minutes(RESET_TOKEN_TTL_MINUTES),
                used_at: None,
                created_at: now,
            })
            .await?;

        let link = format!(
            "{}/reset-password?token={}",
            self.config.app_base_url.trim_end_matches('/'),
            token
        );

        let message = EmailMessage {
            to: user.email.clone(),
            subject: "Reset your password".to_string(),
            body: format!(
                "Hello {},\n\n\
                 Use the link below to choose a new password. It works once and \
                 expires in {} minutes.\n\n\
                 {}\n\n\
                 If you did not ask for this, you can ignore this email — your \
                 password has not changed.",
                user.first_name, RESET_TOKEN_TTL_MINUTES, link
            ),
        };

        // A delivery failure is the operator's problem, not the caller's:
        // surfacing it would answer the enumeration question this endpoint
        // exists to dodge, since only a real account gets this far.
        if let Err(error) = self.email.send(message).await {
            tracing::error!("failed to send a password reset email: {error}");
        }

        acknowledge()
    }

    /// Completes a password reset and ends every existing session.
    pub async fn reset_password(
        &self,
        req: ResetPasswordRequest,
    ) -> AppResult<ResetPasswordResponse> {
        let now = Utc::now();
        let invalid = || AppError::Auth(INVALID_RESET_TOKEN.to_string());

        let token = self
            .reset_tokens
            .find_by_hash(&hash_email_token(&req.token))
            .await?
            .ok_or_else(invalid)?;

        if !token.is_usable(now) {
            return Err(invalid());
        }

        // Spend the token before touching the password: if two requests arrive
        // with the same link, only one of them updates a row here.
        if !self.reset_tokens.mark_used(token.id, now).await? {
            return Err(invalid());
        }

        let user = self.repo.find_by_id(token.user_id).await?.ok_or_else(invalid)?;

        let password_hash = hash_password(&req.password)?;
        self.repo.replace_password(user.id, &password_hash).await?;

        // Any other links already in the user's inbox are now stale.
        self.reset_tokens.expire_all_for_user(user.id, now).await?;

        Ok(ResetPasswordResponse { password_changed: true })
    }

    /// Signs out one session by revoking its refresh token.
    ///
    /// Deliberately unauthenticated: an access token lasts fifteen minutes, and
    /// refusing to sign out the moment one expires would strand exactly the
    /// sessions most in need of ending. Holding the refresh token is itself the
    /// authority to revoke it, and revoking is a safe operation — the worst a
    /// stolen token can do here is end the session it was stolen from.
    ///
    /// The access token issued alongside it is not tracked, so it keeps working
    /// for the remainder of its short life. Making sign-out instant for access
    /// tokens too would mean a denylist lookup on every authenticated request.
    pub async fn logout(&self, req: RefreshTokenRequest) -> AppResult<()> {
        let claims =
            verify_token_of_type(&req.refresh_token, &self.config.jwt_secret, TOKEN_TYPE_REFRESH)?;

        let expires_at = DateTime::from_timestamp(claims.exp, 0)
            .ok_or_else(|| AppError::Auth("Invalid refresh token".to_string()))?;

        self.revoked_tokens.revoke(claims.jti, expires_at).await
    }

    pub async fn get_user_profile(&self, user_id: Uuid) -> AppResult<UserProfile> {
        let user = self.repo.find_by_id(user_id).await?
            .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;
        Ok(user.into())
    }

    /// Admin-only in the handler. `acting_admin_id` is passed so an admin cannot
    /// demote themselves and lock the last administrator out of the instance.
    pub async fn set_user_role(
        &self,
        user_id: Uuid,
        role: &str,
        acting_admin_id: Uuid,
    ) -> AppResult<UserProfile> {
        if user_id == acting_admin_id && role != "admin" {
            return Err(AppError::BadRequest(
                "You cannot remove your own admin role".to_string(),
            ));
        }

        let mut user = self
            .repo
            .find_by_id(user_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("User {} not found", user_id)))?;

        user.role = role.to_string();
        user.updated_at = Utc::now();

        Ok(self.repo.update(&user).await?.into())
    }

    /// Admin-only in the handler. Deactivating is how an account is retired:
    /// deleting it would orphan every document it created.
    pub async fn set_user_status(
        &self,
        user_id: Uuid,
        is_active: bool,
        acting_admin_id: Uuid,
    ) -> AppResult<UserProfile> {
        if user_id == acting_admin_id && !is_active {
            return Err(AppError::BadRequest(
                "You cannot deactivate your own account".to_string(),
            ));
        }

        let mut user = self
            .repo
            .find_by_id(user_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("User {} not found", user_id)))?;

        user.is_active = is_active;
        user.updated_at = Utc::now();

        Ok(self.repo.update(&user).await?.into())
    }

    /// Updates the signed-in user's own name.
    pub async fn update_profile(
        &self,
        user_id: Uuid,
        req: UpdateProfileRequest,
    ) -> AppResult<UserProfile> {
        let mut user = self
            .repo
            .find_by_id(user_id)
            .await?
            .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

        user.first_name = req.first_name;
        user.last_name = req.last_name;
        user.updated_at = Utc::now();

        Ok(self.repo.update(&user).await?.into())
    }

    /// Changes the signed-in user's password, having checked they know the
    /// current one — a logged-in browser someone walked away from should not be
    /// enough to lock the owner out.
    ///
    /// Returns a fresh token pair. The change bumps the session epoch, which
    /// ends every session including this one; reissuing here keeps the person
    /// who made the change signed in while still evicting everyone else.
    pub async fn change_password(
        &self,
        user_id: Uuid,
        req: ChangePasswordRequest,
    ) -> AppResult<AuthResponse> {
        let user = self
            .repo
            .find_by_id(user_id)
            .await?
            .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

        verify_password(&req.current_password, &user.password_hash)
            .map_err(|_| AppError::Auth("Current password is incorrect".to_string()))?;

        let password_hash = hash_password(&req.new_password)?;
        self.repo.replace_password(user.id, &password_hash).await?;

        // Re-read so the new epoch is stamped into the tokens issued below.
        let user = self
            .repo
            .find_by_id(user_id)
            .await?
            .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

        self.create_auth_response(user).await
    }

    async fn create_auth_response(&self, user: User) -> AppResult<AuthResponse> {
        let access_token = create_access_token(
            user.id,
            user.email.clone(),
            user.role.clone(),
            user.org_id,
            user.session_epoch,
            &self.config.jwt_secret,
            self.config.jwt_access_expiry,
        )?;

        let refresh_token = create_refresh_token(
            user.id,
            user.email.clone(),
            user.role.clone(),
            user.org_id,
            user.session_epoch,
            &self.config.jwt_secret,
            self.config.jwt_refresh_expiry,
        )?;

        Ok(AuthResponse {
            access_token,
            refresh_token,
            token_type: "Bearer".to_string(),
            expires_in: self.config.jwt_access_expiry,
            user: UserResponse {
                id: user.id,
                email: user.email,
                first_name: user.first_name,
                last_name: user.last_name,
                role: user.role,
                email_verified: user.email_verified,
            },
        })
    }
}

/// A 256-bit reset token, hex encoded.
///
/// Drawn from the OS generator, not a general-purpose PRNG: this value is the
/// only thing standing between an email address and an account.
fn generate_email_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    to_hex(&bytes)
}

/// SHA-256 of the token, which is what the database stores. Shared by the
/// password reset and email verification flows — same kind of credential, same
/// handling.
///
/// Deliberately not argon2. Slow hashing exists to make guessing a *password*
/// expensive, and buys nothing against 256 bits of randomness — while a lookup
/// by hash needs the same input to produce the same output every time, which a
/// salted hash will not do.
fn hash_email_token(token: &str) -> String {
    to_hex(&Sha256::digest(token.as_bytes()))
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut out, byte| {
        use std::fmt::Write;
        let _ = write!(out, "{byte:02x}");
        out
    })
}

fn hash_password(password: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2.hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|_| AppError::Internal)
}

fn verify_password(password: &str, hash: &str) -> AppResult<()> {
    let parsed_hash = PasswordHash::new(hash)
        .map_err(|_| AppError::Auth("Invalid credentials".to_string()))?;
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .map_err(|_| AppError::Auth("Invalid credentials".to_string()))
}
