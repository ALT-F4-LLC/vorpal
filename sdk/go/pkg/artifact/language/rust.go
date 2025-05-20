package language

import (
	"bytes"
	"fmt"
	"os"
	"slices"
	"strings"
	"text/template"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
	"github.com/BurntSushi/toml"
)

type RustArtifactCargoToml struct {
	Bin       []RustArtifactCargoTomlBinary   `toml:"bin,omitempty"`
	Package   *RustArtifactCargoTomlPackage   `toml:"package,omitempty"`
	Workspace *RustArtifactCargoTomlWorkspace `toml:"workspace,omitempty"`
}

type RustArtifactCargoTomlBinary struct {
	Name string `toml:"name"`
	Path string `toml:"path"`
}

type RustArtifactCargoTomlPackage struct {
	Name string `toml:"name"`
}

type RustArtifactCargoTomlWorkspace struct {
	Members []string `toml:"members,omitempty"`
}

type RustBuilder struct {
	artifacts []*string
	bins      []string
	build     bool
	check     bool
	excludes  []string
	format    bool
	includes  []string
	lint      bool
	name      string
	packages  []string
	source    *string
	tests     bool
	systems   []api.ArtifactSystem
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
{{if .Packages}}
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
{{else}}
mkdir -pv src
touch src/main.rs
{{end}}
mkdir -pv $VORPAL_OUTPUT/vendor

cargo_vendor=$(cargo vendor --versioned-dirs $VORPAL_OUTPUT/vendor)

echo "$cargo_vendor" > $VORPAL_OUTPUT/config.toml`

const StepScriptTemplate = `
mkdir -pv $HOME

pushd ./source/{{.Name}}

mkdir -pv .cargo
mkdir -pv $VORPAL_OUTPUT/bin

ln -sv {{.Vendor}}/config.toml .cargo/config.toml
{{if .Packages}}
cat > Cargo.toml << "EOF"
[workspace]
members = [{{.Packages}}]
resolver = "2"
EOF
{{end}}
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

func NewRustBuilder(name string, systems []api.ArtifactSystem) *RustBuilder {
	return &RustBuilder{
		artifacts: make([]*string, 0),
		bins:      make([]string, 0),
		build:     true,
		check:     false,
		excludes:  make([]string, 0),
		format:    false,
		includes:  make([]string, 0),
		lint:      false,
		name:      name,
		packages:  make([]string, 0),
		source:    nil,
		tests:     false,
		systems:   systems,
	}
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

func (a *RustBuilder) WithIncludes(includes []string) *RustBuilder {
	a.includes = includes
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

func (builder *RustBuilder) Build(context *config.ConfigContext) (*string, error) {
	protoc, err := artifact.Protoc(context)
	if err != nil {
		return nil, err
	}

	// 1. READ CARGO.TOML FILES

	// Get the source path

	sourcePath := "."

	if builder.source != nil {
		sourcePath = *builder.source
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
			packagePath := fmt.Sprintf("%s/%s", sourcePath, member)
			packageCargoPath := fmt.Sprintf("%s/Cargo.toml", packagePath)

			if _, err := os.Stat(packageCargoPath); err != nil {
				return nil, err
			}

			packageCargoData, err := os.ReadFile(packageCargoPath)
			if err != nil {
				return nil, err
			}

			var packageCargo RustArtifactCargoToml

			_, pkgCargoErr := toml.Decode(string(packageCargoData), &packageCargo)
			if pkgCargoErr != nil {
				return nil, pkgCargoErr
			}

			if len(builder.packages) > 0 && !slices.Contains(builder.packages, packageCargo.Package.Name) {
				continue
			}

			packageTargetPaths := make([]string, 0)

			if packageCargo.Bin != nil && len(packageCargo.Bin) > 0 {
				for _, bin := range packageCargo.Bin {
					packageTargetPath := fmt.Sprintf("%s/%s", packagePath, bin.Path)

					if _, err := os.Stat(packageTargetPath); err != nil {
						return nil, err
					}

					packageTargetPaths = append(packageTargetPaths, packageTargetPath)

					if len(builder.bins) == 0 || slices.Contains(builder.bins, bin.Name) {
						if !slices.Contains(packagesManifests, packageCargoPath) {
							packagesManifests = append(packagesManifests, packageCargoPath)
						}

						packagesBinNames = append(packagesBinNames, bin.Name)
					}
				}
			}

			if len(packageTargetPaths) == 0 {
				packageTargetPath := fmt.Sprintf("%s/src/lib.rs", packagePath)

				if _, err := os.Stat(packageTargetPath); err != nil {
					return nil, err
				}

				packageTargetPaths = append(packageTargetPaths, packageTargetPath)
			}

			for _, memberTargetPath := range packageTargetPaths {
				packagesTargets = append(packagesTargets, memberTargetPath)
			}

			packages = append(packages, member)
		}
	}

	// 2. CREATE ARTIFACTS

	// Get rust toolchain artifact

	contextTarget := context.GetTarget()

	rustToolchain, err := artifact.RustToolchain(context)
	if err != nil {
		return nil, err
	}

	rustToolchainTarget, err := artifact.RustToolchainTarget(contextTarget)
	if err != nil {
		return nil, err
	}

	rustToolchainVersion := artifact.RustToolchainVersion()
	rustToolchainName := fmt.Sprintf("%s-%s", rustToolchainVersion, *rustToolchainTarget)

	stepEnvironments := []string{
		"HOME=$VORPAL_WORKSPACE/home",
		fmt.Sprintf(
			"PATH=%s/toolchains/%s/bin",
			artifact.GetEnvKey(rustToolchain),
			rustToolchainName,
		),
		fmt.Sprintf("RUSTUP_HOME=%s", artifact.GetEnvKey(rustToolchain)),
		fmt.Sprintf("RUSTUP_TOOLCHAIN=%s", rustToolchainName),
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
		Name:        builder.name,
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

	vendorName := fmt.Sprintf("%s-vendor", builder.name)

	vendorSource := artifact.NewArtifactSourceBuilder(vendorName, sourcePath).
		WithIncludes(vendorCargoPaths).
		Build()

	vendorSteps := []*api.ArtifactStep{vendorStep}

	systems := []api.ArtifactSystem{
		api.ArtifactSystem_AARCH64_DARWIN,
		api.ArtifactSystem_AARCH64_LINUX,
		api.ArtifactSystem_X8664_DARWIN,
		api.ArtifactSystem_X8664_LINUX,
	}

	vendor, err := artifact.NewArtifactBuilder(vendorName, vendorSteps, systems).
		WithSource(&vendorSource).
		Build(context)
	if err != nil {
		return nil, err
	}

	stepArtifacts = append(stepArtifacts, vendor)
	stepArtifacts = append(stepArtifacts, protoc)

	// TODO: implement artifact for 'check` to pre-bake the vendor cache

	sourceIncludes := make([]string, 0)
	sourceExcludes := make([]string, 0)

	sourceExcludes = append(sourceExcludes, "target")

	for _, exclude := range builder.excludes {
		sourceExcludes = append(sourceExcludes, exclude)
	}

	for _, include := range builder.includes {
		sourceIncludes = append(sourceIncludes, include)
	}

	sourceBuilder := artifact.NewArtifactSourceBuilder(builder.name, sourcePath).
		WithIncludes(sourceIncludes).
		WithExcludes(sourceExcludes)

	source := sourceBuilder.Build()

	for _, artifact := range builder.artifacts {
		stepArtifacts = append(stepArtifacts, artifact)
	}

	stepScript, err := template.New("script").Parse(StepScriptTemplate)
	if err != nil {
		return nil, err
	}

	var stepScriptBuffer bytes.Buffer

	if len(packagesBinNames) == 0 {
		packagesBinNames = []string{builder.name}
	}

	if len(packagesManifests) == 0 {
		packagesManifests = []string{sourceCargoPath}
	}

	stepScriptArgs := StepScriptTemplateArgs{
		BinNames:      strings.Join(packagesBinNames, " "),
		Build:         fmt.Sprintf("%t", builder.build),
		Check:         fmt.Sprintf("%t", builder.check),
		Format:        fmt.Sprintf("%t", builder.format),
		Lint:          fmt.Sprintf("%t", builder.lint),
		ManifestPaths: strings.Join(packagesManifests, " "),
		Name:          builder.name,
		Packages:      strings.Join(stepScriptPackages, ","),
		Tests:         fmt.Sprintf("%t", builder.tests),
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

	steps := []*api.ArtifactStep{step}

	return artifact.NewArtifactBuilder(builder.name, steps, systems).
		WithSource(&source).
		Build(context)
}
