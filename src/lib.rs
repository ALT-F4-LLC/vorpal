pub mod api {
    tonic::include_proto!("vorpal.command.v0");
    tonic::include_proto!("vorpal.package.v0");
}
pub mod database;
pub mod notary;
pub mod service;
pub mod store;
extern crate tera;
