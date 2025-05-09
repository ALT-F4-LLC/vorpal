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
	Workspace *RustArtifactCargoTomlWorkspace `toml:"workspace,omitempty"`
}

type RustArtifactCargoTomlBinary struct {
	Name string `toml:"name"`
	Path string `toml:"path"`
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

func (builder *RustBuilder) Build(context *config.ConfigContext) (*string, error) {
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
			if len(builder.packages) > 0 && !slices.Contains(builder.packages, member) {
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

					if len(builder.bins) == 0 || slices.Contains(builder.bins, bin.Name) {
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

	contextTarget, err := context.GetTarget()
	if err != nil {
		return nil, err
	}

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

	vendor, err := artifact.NewArtifactBuilder(vendorName).
		WithSource(&vendorSource).
		WithStep(vendorStep).
		WithSystem(api.ArtifactSystem_AARCH64_DARWIN).
		WithSystem(api.ArtifactSystem_AARCH64_LINUX).
		WithSystem(api.ArtifactSystem_X8664_DARWIN).
		WithSystem(api.ArtifactSystem_X8664_LINUX).
		Build(context)
	if err != nil {
		return nil, err
	}

	stepArtifacts = append(stepArtifacts, vendor)

	// TODO: implement artifact for 'check` to pre-bake the vendor cache

	sourceIncludes := make([]string, 0)
	sourceExcludes := make([]string, 0)

	sourceExcludes = append(sourceExcludes, "target")

	if len(builder.packages) > 0 {
		for _, pkg := range builder.packages {
			sourceIncludes = append(sourceIncludes, pkg)
		}
	}

	for _, exclude := range builder.excludes {
		sourceExcludes = append(sourceExcludes, exclude)
	}

	sourceBuilder := artifact.NewArtifactSourceBuilder(builder.name, sourcePath)

	if len(sourceIncludes) > 0 {
		sourceBuilder = sourceBuilder.WithIncludes(sourceIncludes)
	} else {
		sourceBuilder = sourceBuilder.WithExcludes(sourceExcludes)
	}

	source := sourceBuilder.Build()

	for _, artifact := range builder.artifacts {
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

	return artifact.NewArtifactBuilder(builder.name).
		WithSource(&source).
		WithStep(step).
		WithSystem(api.ArtifactSystem_AARCH64_DARWIN).
		WithSystem(api.ArtifactSystem_AARCH64_LINUX).
		WithSystem(api.ArtifactSystem_X8664_DARWIN).
		WithSystem(api.ArtifactSystem_X8664_LINUX).
		Build(context)
}
