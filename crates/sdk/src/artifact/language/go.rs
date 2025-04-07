use vorpal_schema::artifact::v0::{
    ArtifactSystem,
    ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
};

pub fn get_goos(target: ArtifactSystem) -> String {
    let goos = match target {
        Aarch64Darwin | X8664Darwin => "darwin",
        Aarch64Linux | X8664Linux => "linux",
        _ => unreachable!(),
    };

    goos.to_string()
}

pub fn get_goarch(target: ArtifactSystem) -> String {
    let goarch = match target {
        Aarch64Darwin | Aarch64Linux => "arm64",
        X8664Darwin => "amd64",
        X8664Linux => "386",
        _ => unreachable!(),
    };

    goarch.to_string()
}
