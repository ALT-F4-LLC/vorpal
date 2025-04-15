package language

import (
	"bytes"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"text/template"

	artifactApi "github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
	"github.com/BurntSushi/toml"
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

type RustShellBuilder struct {
	artifacts []*string
	name      string
}

type RustBuilder struct {
	artifacts []*string
	check     bool
	excludes  []string
	format    bool
	lint      bool
	name      string
	tests     bool
}

type VendorStepScriptTemplateArgs struct {
	Name        string
	TargetPaths string
}

type StepScriptTemplateArgs struct {
	BinNames string
	Check    string
	Format   string
	Lint     string
	Name     string
	Tests    string
	Vendor   string
}

const VendorStepScriptTemplate = `
mkdir -pv $HOME

pushd ./source/{{.Name}}-vendor

target_paths=({{.TargetPaths}})

for target_path in ${{"{"}}target_paths{{"["}}@{{"]"}}{{"}"}}; do
    mkdir -pv "$(dirname "${{"{"}}target_path{{"}"}}")"
    touch "${{"{"}}target_path{{"}"}}"
done

mkdir -pv "$VORPAL_OUTPUT/vendor"

cargo_vendor=$(cargo vendor --versioned-dirs $VORPAL_OUTPUT/vendor)

echo "$cargo_vendor" > "$VORPAL_OUTPUT/config.toml"`

const StepScriptTemplate = `
mkdir -pv $HOME

pushd ./source/{{.Name}}

mkdir -pv .cargo

ln -sv "{{.Vendor}}/config.toml" .cargo/config.toml

if [ "{{.Check}}" = "true" ]; then
    cargo --offline check --release
fi

if [ "{{.Format}}" = "true" ]; then
    cargo --offline fmt --all --check
fi

if [ "{{.Lint}}" = "true" ]; then
    cargo --offline clippy -- --deny warnings
fi

cargo --offline build --release

if [ "{{.Tests}}" = "true" ]; then
    cargo --offline test --release
fi

mkdir -pv "$VORPAL_OUTPUT/bin"

bin_names=({{.BinNames}})

for bin_name in ${{"{"}}bin_names{{"["}}@{{"]"}}{{"}"}}; do
    cp -pv "target/release/${{"{"}}bin_name{{"}"}}" "$VORPAL_OUTPUT/bin/"
done`

func toolchain_digest(context *config.ConfigContext) (*string, error) {
	target := context.GetTarget()

	var digest string

	switch target {
	case artifactApi.ArtifactSystem_AARCH64_DARWIN:
		digest = "84707c7325d3a0cbd8044020a5256b6fd43a79bd837948bb4a7e90d671c919e6"
	case artifactApi.ArtifactSystem_AARCH64_LINUX:
		digest = "ad490acd52f5b4d5b539df8f565df3a90271225a1ef6256c1027eac0b70cb4d4"
	case artifactApi.ArtifactSystem_X8664_DARWIN:
		digest = "589c625bd79be3ed8b9d5168c54a889dba971a6e9d9722750c4b4577247ec94e"
	case artifactApi.ArtifactSystem_X8664_LINUX:
		digest = "5442c5e085972b7119661da12d03d40fb17770edf8879ab898aee3dafdd1c48c"
	default:
		return nil, errors.New("unsupported target")
	}

	return &digest, nil
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

func NewRustShellBuilder(name string) *RustShellBuilder {
	return &RustShellBuilder{
		artifacts: make([]*string, 0),
		name:      name,
	}
}

func NewRustBuilder(name string) *RustBuilder {
	return &RustBuilder{
		artifacts: make([]*string, 0),
		check:     false,
		excludes:  make([]string, 0),
		format:    false,
		lint:      false,
		name:      name,
		tests:     false,
	}
}

func (a *RustShellBuilder) WithArtifacts(artifacts []*string) *RustShellBuilder {
	a.artifacts = artifacts
	return a
}

func (a *RustShellBuilder) Build(context *config.ConfigContext) (*string, error) {
	artifacts := make([]*string, 0)

	toolchain_digest, err := toolchain_digest(context)
	if err != nil {
		return nil, err
	}

	toolchain, err := context.FetchArtifact(*toolchain_digest)
	if err != nil {
		return nil, err
	}

	toolchain_target, err := toolchain_target(context.GetTarget())
	if err != nil {
		return nil, err
	}

	toolchain_version := toolchain_version()

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

	return artifact.ShellArtifact(context, artifacts, environments, a.name)
}

func (a *RustBuilder) WithArtifacts(artifacts []*string) *RustBuilder {
	a.artifacts = artifacts
	return a
}

func (a *RustBuilder) WithCheck() *RustBuilder {
	a.check = true
	return a
}

func (a *RustBuilder) WithExcludes(excludes []string) *RustBuilder {
	a.excludes = excludes
	return a
}

func (a *RustBuilder) WithFormat() *RustBuilder {
	a.format = true
	return a
}

func (a *RustBuilder) WithLint() *RustBuilder {
	a.lint = true
	return a
}

func (a *RustBuilder) WithTests() *RustBuilder {
	a.tests = true
	return a
}

func (a *RustBuilder) Build(context *config.ConfigContext) (*string, error) {
	// 1. READ CARGO.TOML FILES

	// Get the source path

	sourcePath := filepath.Dir(".")

	// Load root cargo.toml

	sourceCargoPath := filepath.Join(sourcePath, "Cargo.toml")

	if _, err := os.Stat(sourceCargoPath); err != nil {
		if os.IsNotExist(err) {
			return nil, fmt.Errorf("cargo.toml file not found in %s", sourcePath)
		}
		return nil, err
	}

	sourceCargoData, err := os.ReadFile(sourceCargoPath)
	if err != nil {
		return nil, err
	}

	var sourceCargo RustArtifactCargoToml

	_, cargoTomlErr := toml.Decode(string(sourceCargoData), &sourceCargo)
	if cargoTomlErr != nil {
		return nil, cargoTomlErr
	}

	workspaces := make([]string, 0)
	workspacesBinNames := make([]string, 0)
	workspacesTargets := make([]string, 0)

	if sourceCargo.Workspace != nil && len(sourceCargo.Workspace.Members) > 0 {
		for _, member := range sourceCargo.Workspace.Members {
			memberPath := filepath.Join(sourcePath, member)
			memberCargoTomlPath := filepath.Join(memberPath, "Cargo.toml")

			if _, err := os.Stat(memberCargoTomlPath); err != nil {
				return nil, err
			}

			memberTomlData, err := os.ReadFile(memberCargoTomlPath)
			if err != nil {
				return nil, err
			}

			var memberCargoToml RustArtifactCargoToml

			_, memberCargoTomlErr := toml.Decode(string(memberTomlData), &memberCargoToml)
			if memberCargoTomlErr != nil {
				return nil, memberCargoTomlErr
			}

			memberTargetPaths := make([]string, 0)

			if memberCargoToml.Bin != nil && len(memberCargoToml.Bin) > 0 {
				for _, bin := range memberCargoToml.Bin {
					memberTargetPath := filepath.Join(memberPath, bin.Path)

					if _, err := os.Stat(memberTargetPath); err != nil {
						return nil, err
					}

					memberTargetPaths = append(memberTargetPaths, memberTargetPath)
					workspacesBinNames = append(workspacesBinNames, bin.Name)
				}
			}

			if len(memberTargetPaths) == 0 {
				memberTargetPath := filepath.Join(memberPath, "src/lib.rs")

				if _, err := os.Stat(memberTargetPath); err != nil {
					return nil, err
				}

				memberTargetPaths = append(memberTargetPaths, memberTargetPath)
			}

			for _, memberTargetPath := range memberTargetPaths {
				workspacesTargets = append(workspacesTargets, memberTargetPath)
			}

			workspaces = append(workspaces, member)
		}
	}

	// 2. CREATE ARTIFACTS

	// Get rust toolchain artifact

	rust_toolchain_digest, err := toolchain_digest(context)
	if err != nil {
		return nil, err
	}

	rustToolchain, err := context.FetchArtifact(*rust_toolchain_digest)
	if err != nil {
		return nil, err
	}

	rust_toolchain_target, err := toolchain_target(context.GetTarget())
	if err != nil {
		return nil, err
	}

	rust_toolchain_version := toolchain_version()

	rust_toolchain_name := fmt.Sprintf("%s-%s", rust_toolchain_version, *rust_toolchain_target)

	stepEnvironments := []string{
		"HOME=$VORPAL_WORKSPACE/home",
		fmt.Sprintf(
			"PATH=%s/toolchains/%s/bin",
			artifact.GetEnvKey(rustToolchain),
			rust_toolchain_name,
		),
		fmt.Sprintf("RUSTUP_HOME=%s", artifact.GetEnvKey(rustToolchain)),
		fmt.Sprintf("RUSTUP_TOOLCHAIN=%s", rust_toolchain_name),
	}

	// Create vendor artifact

	vendorCargoPaths := []string{"Cargo.toml", "Cargo.lock"}

	for _, workspace := range workspaces {
		vendorCargoPaths = append(vendorCargoPaths, filepath.Join(workspace, "Cargo.toml"))
	}

	vendorStepScript, err := template.New("script").Parse(VendorStepScriptTemplate)
	if err != nil {
		return nil, err
	}

	var vendorStepScriptBuffer bytes.Buffer

	vendorStepScriptTargetPaths := strings.Join(workspacesTargets, " ")

	vendorStepScriptArgs := VendorStepScriptTemplateArgs{
		Name:        a.name,
		TargetPaths: vendorStepScriptTargetPaths,
	}

	if err := vendorStepScript.Execute(&vendorStepScriptBuffer, vendorStepScriptArgs); err != nil {
		return nil, err
	}

	stepArtifacts := []*string{rustToolchain}

	vendorStep, err := artifact.Shell(
		context,
		stepArtifacts,
		stepEnvironments,
		vendorStepScriptBuffer.String(),
	)

	vendorName := fmt.Sprintf("%s-vendor", a.name)

	vendorSource := artifact.NewArtifactSourceBuilder(vendorName, sourcePath).
		WithIncludes(vendorCargoPaths).
		Build()

	vendor, err := artifact.NewArtifactBuilder(vendorName).
		WithSource(&vendorSource).
		WithStep(vendorStep).
		WithSystem(artifactApi.ArtifactSystem_AARCH64_DARWIN).
		WithSystem(artifactApi.ArtifactSystem_AARCH64_LINUX).
		WithSystem(artifactApi.ArtifactSystem_X8664_DARWIN).
		WithSystem(artifactApi.ArtifactSystem_X8664_LINUX).
		Build(context)
	if err != nil {
		return nil, err
	}

	stepArtifacts = append(stepArtifacts, vendor)

	// TODO: implement artifact for 'check` to pre-bake the vendor cache

	sourceExcludes := make([]string, 0)

	sourceExcludes = append(sourceExcludes, "target")

	for _, exclude := range a.excludes {
		sourceExcludes = append(sourceExcludes, exclude)
	}

	source := artifact.NewArtifactSourceBuilder(a.name, sourcePath).
		WithExcludes(sourceExcludes).
		Build()

	for _, artifact := range a.artifacts {
		stepArtifacts = append(stepArtifacts, artifact)
	}

	stepScript, err := template.New("script").Parse(StepScriptTemplate)
	if err != nil {
		return nil, err
	}

	var stepScriptBuffer bytes.Buffer

	stepScriptBinNames := strings.Join(workspacesBinNames, " ")

	stepScriptArgs := StepScriptTemplateArgs{
		BinNames: stepScriptBinNames,
		Check:    fmt.Sprintf("%t", a.check),
		Format:   fmt.Sprintf("%t", a.format),
		Lint:     fmt.Sprintf("%t", a.lint),
		Name:     a.name,
		Tests:    fmt.Sprintf("%t", a.tests),
		Vendor:   artifact.GetEnvKey(vendor),
	}

	if err := stepScript.Execute(&stepScriptBuffer, stepScriptArgs); err != nil {
		return nil, err
	}

	step, err := artifact.Shell(
		context,
		stepArtifacts,
		stepEnvironments,
		stepScriptBuffer.String(),
	)
	if err != nil {
		return nil, err
	}

	return artifact.NewArtifactBuilder(a.name).
		WithSource(&source).
		WithStep(step).
		WithSystem(artifactApi.ArtifactSystem_AARCH64_DARWIN).
		WithSystem(artifactApi.ArtifactSystem_AARCH64_LINUX).
		WithSystem(artifactApi.ArtifactSystem_X8664_DARWIN).
		WithSystem(artifactApi.ArtifactSystem_X8664_LINUX).
		Build(context)
}
