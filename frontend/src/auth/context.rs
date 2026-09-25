//! Global authentication context.
//!
//! Provides a reactive user signal that every page can read. The
//! context is created once in `crate::app::App` via `provide_auth()`
//! and read with `use_auth()`.

use leptos::prelude::*;
use uuid::Uuid;

use crate::api::models::UserResponse;
use crate::api::ApiClient;
use crate::auth::storage;

/// The authenticated user as seen by the frontend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedUser {
    pub id: Uuid,
    pub email: String,
    pub full_name: String,
}

impl From<UserResponse> for AuthenticatedUser {
    fn from(u: UserResponse) -> Self {
        Self {
            id: u.id,
            email: u.email,
            full_name: u.full_name,
        }
    }
}

/// The auth context, installed at the root of the component tree.
#[derive(Clone, Copy)]
pub struct AuthContext {
    pub user: RwSignal<Option<AuthenticatedUser>>,
    pub client: RwSignal<ApiClient>,
    /// True once the initial session bootstrap has finished (success or
    /// failure). `ProtectedRoute` must wait for this before redirecting
    /// to `/login` — otherwise a page refresh would briefly log the user
    /// out before `GET /api/auth/me` returns.
    pub bootstrap_done: RwSignal<bool>,
}

impl AuthContext {
    /// Stores the token and installs the user. Call after a successful
    /// login or register.
    pub fn install_session(&self, user: AuthenticatedUser, token: String) {
        storage::write_token(&token);
        self.client.set(ApiClient::with_token(token));
        self.user.set(Some(user));
        self.bootstrap_done.set(true);
    }

    /// Clears local auth state. Does **not** call the backend — callers
    /// that want server-side revocation must call `ApiClient::logout`
    /// first.
    pub fn clear(&self) {
        storage::clear_token();
        self.client.set(ApiClient::new());
        self.user.set(None);
        // Mark bootstrap as done so guard does not wait forever after
        // an explicit sign-out.
        self.bootstrap_done.set(true);
    }

    /// True if a user is currently installed in the context.
    pub fn is_authenticated(&self) -> bool {
        self.user.get().is_some()
    }
}

/// Installs the auth context into the current reactive scope.
pub fn provide_auth() -> AuthContext {
    let token = storage::read_token();
    let client = match &token {
        Some(t) => ApiClient::with_token(t.clone()),
        None => ApiClient::new(),
    };
    // If there is no stored token there is nothing to bootstrap, so we
    // are "done" immediately. If there is a token, `SessionBootstrap`
    // will set this to `true` after `/api/auth/me` returns.
    let bootstrap_done = RwSignal::new(token.is_none());
    let ctx = AuthContext {
        user: RwSignal::new(None),
        client: RwSignal::new(client),
        bootstrap_done,
    };
    provide_context(ctx);
    ctx
}

/// Reads the auth context from the reactive scope.
///
/// Panics if `provide_auth()` was not called higher in the tree.
pub fn use_auth() -> AuthContext {
    use_context::<AuthContext>().expect("AuthContext not provided; call provide_auth() in App")
}
