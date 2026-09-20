//! Spawners for the platform-level tests (auth, launcher, layout).
//!
//! The work — database, migrations, scoped pools, config, router — lives in
//! `myapps-test-harness`, shared with every app crate's tests. These are only
//! the platform-specific entry points: they always register *every* app, which
//! the app crates never do.

use myapps_core::config::ExternalApp;
use myapps_test_harness::{Options, TestApp, spawn_app_with};

/// Spin up a fresh instance with every app registered.
pub async fn spawn_app() -> TestApp {
    spawn_app_with(myapps::all_app_instances(), Options::default()).await
}

/// Spin up a fresh instance limited to a subset of apps, as `DEPLOY_APPS` does.
pub async fn spawn_app_with_deploy_apps(deploy_apps: Option<Vec<String>>) -> TestApp {
    spawn_app_with(
        myapps::all_app_instances(),
        Options {
            deploy_apps,
            ..Options::default()
        },
    )
    .await
}

/// Spin up a fresh instance with external app shortcuts on the launcher.
pub async fn spawn_app_with_external_apps(external_apps: Vec<ExternalApp>) -> TestApp {
    spawn_app_with(
        myapps::all_app_instances(),
        Options {
            external_apps,
            ..Options::default()
        },
    )
    .await
}

/// Spin up a fresh instance with version info, for footer tests.
pub async fn spawn_app_with_version(version: &str, build_timestamp: &str) -> TestApp {
    spawn_app_with(
        myapps::all_app_instances(),
        Options {
            version: version.into(),
            build_timestamp: build_timestamp.into(),
            ..Options::default()
        },
    )
    .await
}

/// Spin up a fresh instance with reverse-proxy SSO authentication enabled.
pub async fn spawn_app_with_sso() -> TestApp {
    spawn_app_with(
        myapps::all_app_instances(),
        Options {
            auth_sso_header: Some("Remote-User".into()),
            ..Options::default()
        },
    )
    .await
}
