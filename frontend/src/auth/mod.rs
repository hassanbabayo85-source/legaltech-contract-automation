//! Authentication: token storage, reactive context, route guard.
//!
//! The frontend never authenticates on its own. It stores the bearer
//! token the backend issues, attaches it to requests, and clears it
//! when the backend reports 401 Unauthorized or when the user logs
//! out.

pub mod context;
pub mod guard;
pub mod storage;

pub use context::{provide_auth, use_auth, AuthContext, AuthenticatedUser};
pub use guard::ProtectedRoute;
