package language

import (
	"bytes"
	"errors"
	"fmt"
	"os"
	"slices"
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
	bins      []string
	build     bool
	check     bool
	excludes  []string
	format    bool
	lint      bool
	name      string
	packages  []string
	source    *string
	tests     bool
}

type VendorStepScriptTemplateArgs struct {
	Name        string
	Packages    string
	TargetPaths string
}

type StepScriptTemplateArgs struct {
	BinNames      string
	Build         string
	Check         string
	Format        string
	Lint          string
	ManifestPaths string
	Name          string
	Packages      string
	Tests         string
	Vendor        string
}

const VendorStepScriptTemplate = `
mkdir -pv $HOME

pushd ./source/{{.Name}}-vendor

cat > Cargo.toml << "EOF"
[workspace]
members = [{{.Packages}}]
resolver = "2"
EOF

target_paths=({{.TargetPaths}})

for target_path in ${{"{"}}target_paths{{"["}}@{{"]"}}{{"}"}}; do
    mkdir -pv $(dirname ${{"{"}}target_path{{"}"}})
    touch ${{"{"}}target_path{{"}"}}
done

mkdir -pv $VORPAL_OUTPUT/vendor

cargo_vendor=$(cargo vendor --versioned-dirs $VORPAL_OUTPUT/vendor)

echo "$cargo_vendor" > $VORPAL_OUTPUT/config.toml`

const StepScriptTemplate = `
mkdir -pv $HOME

pushd ./source/{{.Name}}

mkdir -pv .cargo
mkdir -pv $VORPAL_OUTPUT/bin

ln -sv {{.Vendor}}/config.toml .cargo/config.toml

cat > Cargo.toml << "EOF"
[workspace]
members = [{{.Packages}}]
resolver = "2"
EOF

bin_names=({{.BinNames}})
manifest_paths=({{.ManifestPaths}})

if [ "{{.Format}}" = "true" ]; then
    echo "Running formatter..."
    cargo --offline fmt --all --check
fi

for manifest_path in ${{"{"}}manifest_paths{{"["}}@{{"]"}}{{"}"}}; do
    if [ "{{.Lint}}" = "true" ]; then
        echo "Running linter..."
        cargo --offline clippy --manifest-path ${{"{"}}manifest_path{{"}"}} -- --deny warnings
    fi
done

for bin_name in ${{"{"}}bin_names{{"["}}@{{"]"}}{{"}"}}; do
    if [ "{{.Check}}" = "true" ]; then
        echo "Running check..."
        cargo --offline check --bin ${{"{"}}bin_name{{"}"}} --release
    fi

    if [ "{{.Build}}" = "true" ]; then
        echo "Running build..."
        cargo --offline build --bin ${{"{"}}bin_name{{"}"}} --release
    fi

    if [ "{{.Tests}}" = "true" ]; then
        echo "Running tests..."
        cargo --offline test --bin ${{"{"}}bin_name{{"}"}} --release
    fi

    cp -pv ./target/release/${{"{"}}bin_name{{"}"}} $VORPAL_OUTPUT/bin/
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
		bins:      make([]string, 0),
		build:     true,
		check:     false,
		excludes:  make([]string, 0),
		format:    false,
		lint:      false,
		name:      name,
		packages:  make([]string, 0),
		source:    nil,
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

	return artifact.ScriptDevshell(context, artifacts, environments, a.name)
}

func (a *RustBuilder) WithArtifacts(artifacts []*string) *RustBuilder {
	a.artifacts = artifacts
	return a
}

func (a *RustBuilder) WithBins(bins []string) *RustBuilder {
	a.bins = bins
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

func (a *RustBuilder) WithPackages(packages []string) *RustBuilder {
	a.packages = packages
	return a
}

func (a *RustBuilder) WithSource(source *string) *RustBuilder {
	a.source = source
	return a
}

func (a *RustBuilder) WithTests() *RustBuilder {
	a.tests = true
	return a
}

func (a *RustBuilder) Build(context *config.ConfigContext) (*string, error) {
	// 1. READ CARGO.TOML FILES

	// Get the source path

	sourcePath := "."

	if a.source != nil {
		sourcePath = *a.source
	}

	// Load root cargo.toml

	sourceCargoPath := fmt.Sprintf("%s/Cargo.toml", sourcePath)

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

	packages := make([]string, 0)
	packagesBinNames := make([]string, 0)
	packagesManifests := make([]string, 0)
	packagesTargets := make([]string, 0)

	if sourceCargo.Workspace != nil && len(sourceCargo.Workspace.Members) > 0 {
		for _, member := range sourceCargo.Workspace.Members {
			if len(a.packages) > 0 && !slices.Contains(a.packages, member) {
				continue
			}

			pkg := fmt.Sprintf("%s/%s", sourcePath, member)
			pkgCargoPath := fmt.Sprintf("%s/Cargo.toml", pkg)

			if _, err := os.Stat(pkgCargoPath); err != nil {
				return nil, err
			}

			pkgCargoData, err := os.ReadFile(pkgCargoPath)
			if err != nil {
				return nil, err
			}

			var pkgCargo RustArtifactCargoToml

			_, pkgCargoErr := toml.Decode(string(pkgCargoData), &pkgCargo)
			if pkgCargoErr != nil {
				return nil, pkgCargoErr
			}

			pkgTargetPaths := make([]string, 0)

			if pkgCargo.Bin != nil && len(pkgCargo.Bin) > 0 {
				for _, bin := range pkgCargo.Bin {
					pkgTargetPath := fmt.Sprintf("%s/%s", pkg, bin.Path)

					if _, err := os.Stat(pkgTargetPath); err != nil {
						return nil, err
					}

					pkgTargetPaths = append(pkgTargetPaths, pkgTargetPath)

					if len(a.bins) == 0 || slices.Contains(a.bins, bin.Name) {
						if !slices.Contains(packagesManifests, pkgCargoPath) {
							packagesManifests = append(packagesManifests, pkgCargoPath)
						}

						packagesBinNames = append(packagesBinNames, bin.Name)
					}
				}
			}

			if len(pkgTargetPaths) == 0 {
				pkgTargetPath := fmt.Sprintf("%s/src/lib.rs", pkg)

				if _, err := os.Stat(pkgTargetPath); err != nil {
					return nil, err
				}

				pkgTargetPaths = append(pkgTargetPaths, pkgTargetPath)
			}

			for _, memberTargetPath := range pkgTargetPaths {
				packagesTargets = append(packagesTargets, memberTargetPath)
			}

			packages = append(packages, member)
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

	for _, workspace := range packages {
		vendorCargoPaths = append(vendorCargoPaths, fmt.Sprintf("%s/Cargo.toml", workspace))
	}

	vendorStepScript, err := template.New("script").Parse(VendorStepScriptTemplate)
	if err != nil {
		return nil, err
	}

	var vendorStepScriptBuffer bytes.Buffer

	stepPackagesTargets := make([]string, 0)
	stepScriptPackages := make([]string, 0)

	for _, pkg := range packagesTargets {
		stepPackagesTargets = append(stepPackagesTargets, fmt.Sprintf("\"%s\"", pkg))
	}

	for _, pkg := range packages {
		stepScriptPackages = append(stepScriptPackages, fmt.Sprintf("\"%s\"", pkg))
	}

	vendorStepScriptArgs := VendorStepScriptTemplateArgs{
		Name:        a.name,
		Packages:    strings.Join(stepScriptPackages, ","),
		TargetPaths: strings.Join(stepPackagesTargets, " "),
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

	sourceIncludes := make([]string, 0)
	sourceExcludes := make([]string, 0)

	sourceExcludes = append(sourceExcludes, "target")

	if len(a.packages) > 0 {
		for _, pkg := range a.packages {
			sourceIncludes = append(sourceIncludes, pkg)
		}
	}

	for _, exclude := range a.excludes {
		sourceExcludes = append(sourceExcludes, exclude)
	}

	sourceBuilder := artifact.NewArtifactSourceBuilder(a.name, sourcePath)

	if len(sourceIncludes) > 0 {
		sourceBuilder = sourceBuilder.WithIncludes(sourceIncludes)
	} else {
		sourceBuilder = sourceBuilder.WithExcludes(sourceExcludes)
	}

	source := sourceBuilder.Build()

	for _, artifact := range a.artifacts {
		stepArtifacts = append(stepArtifacts, artifact)
	}

	stepScript, err := template.New("script").Parse(StepScriptTemplate)
	if err != nil {
		return nil, err
	}

	var stepScriptBuffer bytes.Buffer

	stepScriptBinNames := strings.Join(packagesBinNames, " ")

	stepScriptArgs := StepScriptTemplateArgs{
		BinNames:      stepScriptBinNames,
		Build:         fmt.Sprintf("%t", a.build),
		Check:         fmt.Sprintf("%t", a.check),
		Format:        fmt.Sprintf("%t", a.format),
		Lint:          fmt.Sprintf("%t", a.lint),
		ManifestPaths: strings.Join(packagesManifests, " "),
		Name:          a.name,
		Packages:      strings.Join(stepScriptPackages, ","),
		Tests:         fmt.Sprintf("%t", a.tests),
		Vendor:        artifact.GetEnvKey(vendor),
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
