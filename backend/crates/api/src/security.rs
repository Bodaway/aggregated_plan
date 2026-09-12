//! CSRF defense for `POST /graphql`.
//!
//! `CorsLayer` only decides who is allowed to *read* a cross-origin response --
//! it never stops a request from being sent and executed. A CORS "simple
//! request" (e.g. a POST with `Content-Type: text/plain`) never triggers a
//! preflight, so it reaches `graphql_handler` regardless of `Origin`, and
//! async-graphql's `receive_batch_body` falls through every non-multipart
//! content type to `receive_batch_json` -- it does not care that the
//! `Content-Type` claims `text/plain`. Combined with the fact that
//! `graphql_handler` authenticates nothing per request (every request resolves
//! against a hardcoded `default_user_id`), reachability equals authority: any
//! page the user has open in a browser tab could fire blind GraphQL mutations
//! at `127.0.0.1:3001`. Concretely, `updateConfiguration` accepts an arbitrary
//! key with no allow-list, so a page can point `gryzzly.base_url` (or
//! `jira.base_url`) at an attacker-controlled host and then call
//! `triggerSync`, which sends a token derived from the user's session cookie to
//! that host. This also closes a DNS-rebinding variant of the same hole: with
//! no `Host` validation, a domain rebound to `127.0.0.1` is treated as
//! same-origin and `CorsLayer` never even engages.
//!
//! The fix: require a non-safelisted request header on every `POST /graphql`.
//! Setting any header outside the CORS-safelisted set forces the browser to run
//! a preflight (`OPTIONS`) before the real request, and the existing origin
//! allow-list in `CorsLayer` adjudicates that preflight -- a cross-site page
//! cannot forge this header without first passing it. The header's value is
//! **not a secret and must never become one**: its entire security value is
//! that a cross-origin page cannot attach it before preflight succeeds, not
//! that it is hard to guess. Do not "harden" this into a bearer token or an
//! API key -- that would just move the secret into client-side JS, reachable by
//! the same attacker page this header is meant to keep out.
//!
//! This does **not** close a DNS-rebinding variant of the same hole. If a
//! domain is rebound mid-session to resolve to `127.0.0.1`, the browser sees
//! scheme+host+port unchanged and treats the request as same-origin: no CORS
//! runs, no preflight is triggered, and script may set any non-forbidden
//! header on a same-origin request -- including this one, whose name is
//! public (this file, the bundled frontend, the served GraphiQL HTML). The
//! middleware would see the header, pass the request through, and because
//! the response is same-origin the attacker page can read it too. Only
//! `Host` header validation closes that variant, and it is not implemented
//! here.

use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

/// Header whose mere *presence* (any value) proves the request already cleared
/// a CORS preflight. See the module doc for why presence, not content, is what
/// matters here.
pub const CSRF_HEADER_NAME: &str = "x-aplan-client";

/// Rejects the request unless [`CSRF_HEADER_NAME`] is set. Intended to be
/// layered onto the `POST /graphql` route only -- see `main.rs` -- so it never
/// touches the OAuth `GET` redirects or the debug GraphiQL playground route.
pub async fn require_csrf_header(req: Request, next: Next) -> Response {
    if req.headers().contains_key(CSRF_HEADER_NAME) {
        next.run(req).await
    } else {
        (StatusCode::FORBIDDEN, "missing required client header").into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::{Request as HttpRequest, StatusCode};
    use tower::ServiceExt;

    // `test_app_state()` vit maintenant dans `crate::test_support` (main.rs) :
    // ce module de tests et les nouveaux tests du service statique dans
    // `main.rs` ont besoin du même `AppState`, donc plus propre de le
    // construire à un seul endroit que de le dupliquer ici.
    use crate::test_support::test_app_state;

    /// Text the middleware itself returns on rejection. Asserting on this (not
    /// just the status code) tells a 403 produced by `require_csrf_header`
    /// apart from a coincidental 403 the handler itself might one day return.
    const REJECTION_BODY: &str = "missing required client header";

    #[tokio::test]
    async fn rejects_post_graphql_without_header() {
        let req = HttpRequest::post("/graphql")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"query":"{ __typename }"}"#))
            .unwrap();
        let res = crate::build_router(test_app_state().await, None)
            .oneshot(req)
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
        let body = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        // Proves the middleware short-circuited the request -- this is the
        // middleware's own rejection text, not anything `graphql_handler` or
        // async-graphql could produce.
        assert_eq!(body, REJECTION_BODY.as_bytes());
    }

    #[tokio::test]
    async fn allows_post_graphql_with_header() {
        let req = HttpRequest::post("/graphql")
            .header(CSRF_HEADER_NAME, "1")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"query":"{ __typename }"}"#))
            .unwrap();
        let res = crate::build_router(test_app_state().await, None)
            .oneshot(req)
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        // Proves the request reached `graphql_handler` and the schema
        // actually executed the query -- not just "not a 403".
        assert_eq!(
            body,
            r#"{"data":{"__typename":"CombinedQuery"}}"#.as_bytes()
        );
    }

    #[tokio::test]
    async fn oauth_get_routes_are_unaffected() {
        let req = HttpRequest::get("/auth/microsoft/login")
            .body(Body::empty())
            .unwrap();
        let res = crate::build_router(test_app_state().await, None)
            .oneshot(req)
            .await
            .unwrap();
        // `auth::microsoft::login` always responds with a temporary redirect;
        // asserting the exact status (not just "!= FORBIDDEN") proves the
        // route ran normally rather than failing open for an unrelated reason.
        assert_eq!(res.status(), StatusCode::TEMPORARY_REDIRECT);
    }

    /// The actual attack shape the whole fix exists for: a CORS "simple
    /// request" is exactly a POST whose `Content-Type` is one of
    /// `text/plain`, `application/x-www-form-urlencoded` or
    /// `multipart/form-data`, sent with no custom header -- the browser skips
    /// preflight entirely for these, and (per this module's doc comment)
    /// async-graphql's body extractor doesn't care that the `Content-Type`
    /// claims `text/plain`. If this test ever went green while the middleware
    /// were absent, the vulnerability this module exists to close would still
    /// be open.
    #[tokio::test]
    async fn rejects_simple_request_shaped_attack() {
        let req = HttpRequest::post("/graphql")
            .header("content-type", "text/plain")
            .body(Body::from(r#"{"query":"{ __typename }"}"#))
            .unwrap();
        let res = crate::build_router(test_app_state().await, None)
            .oneshot(req)
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
        let body = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        assert_eq!(body, REJECTION_BODY.as_bytes());
    }
}
