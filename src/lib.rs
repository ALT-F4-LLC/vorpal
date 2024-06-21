pub mod api {
    tonic::include_proto!("vorpal.config.v0");
    tonic::include_proto!("vorpal.package.v0");
    tonic::include_proto!("vorpal.store.v0");
}
pub mod notary;
pub mod service;
pub mod store;
extern crate tera;
