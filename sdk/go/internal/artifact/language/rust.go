package language

import (
	"errors"
	"fmt"

	artifactApi "github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

type RustArtifactCargoToml struct {
	Bin       []RustArtifactCargoTomlBinary   `toml:"bin,omitempty"`
	Workspace *RustArtifactCargoTomlWorkspace `toml:"workspace,omitempty"`
}

type RustArtifactCargoTomlBinary struct {
	Name string `toml:"name"`
	Path string `toml:"path"`
}

type RustArtifactCargoTomlWorkspace struct {
	Members []string `toml:"members,omitempty"`
}

func toolchain_digest() string {
	return "84707c7325d3a0cbd8044020a5256b6fd43a79bd837948bb4a7e90d671c919e6"
}

func toolchain_target(target artifactApi.ArtifactSystem) (*string, error) {
	aarch64Darwin := "aarch64-apple-darwin"
	aarch64Linux := "aarch64-unknown-linux-gnu"
	x8664Darwin := "x86_64-apple-darwin"
	x8664Linux := "x86_64-unknown-linux-gnu"

	switch target {
	case artifactApi.ArtifactSystem_AARCH64_DARWIN:
		return &aarch64Darwin, nil
	case artifactApi.ArtifactSystem_AARCH64_LINUX:
		return &aarch64Linux, nil
	case artifactApi.ArtifactSystem_X8664_DARWIN:
		return &x8664Darwin, nil
	case artifactApi.ArtifactSystem_X8664_LINUX:
		return &x8664Linux, nil
	default:
		return nil, errors.New("unsupported target")
	}
}

func toolchain_version() string {
	return "1.83.0"
}

type RustShellBuilder struct {
	artifacts []*string
	name      string
}

func NewRustShellBuilder(name string) *RustShellBuilder {
	return &RustShellBuilder{
		artifacts: make([]*string, 0),
		name:      name,
	}
}

func (a *RustShellBuilder) WithArtifacts(artifacts []*string) *RustShellBuilder {
	a.artifacts = artifacts
	return a
}

func (a *RustShellBuilder) Build(content *config.ConfigContext) (*string, error) {
	artifacts := make([]*string, 0)

	toolchain_digest := toolchain_digest()

	toolchain, err := content.FetchArtifact(toolchain_digest)
	if err != nil {
		return nil, err
	}

	toolchain_version := toolchain_version()

	toolchain_target, err := toolchain_target(content.GetTarget())
	if err != nil {
		return nil, err
	}

	artifacts = append(artifacts, toolchain)
	artifacts = append(artifacts, a.artifacts...)

	environments := []string{
		fmt.Sprintf(
			"PATH=%s/toolchains/%s-%s/bin",
			artifact.GetEnvKey(toolchain),
			toolchain_version,
			*toolchain_target,
		),
		fmt.Sprintf("RUSTUP_HOME=%s", artifact.GetEnvKey(toolchain)),
		fmt.Sprintf("RUSTUP_TOOLCHAIN=%s-%s", toolchain_version, *toolchain_target),
	}

	return artifact.ShellArtifact(content, artifacts, environments, a.name)
}
