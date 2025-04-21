package main

import (
	"bytes"
	"fmt"
	"log"
	"text/template"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact/language"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

type releaseScriptArgs struct {
	Aarch64Darwin string
	Aarch64Linux  string
	BranchName    string
	X8664Darwin   string
	X8664Linux    string
}

const releaseScript = `
git clone \
    --branch {{.BranchName}} \
    --depth 1 \
    git@github.com:ALT-F4-LLC/vorpal.git

pushd vorpal

git fetch --tags
git tag --delete nightly || true
git push origin :refs/tags/nightly || true
gh release delete --yes nightly || true

git tag nightly
git push --tags

gh release create \
    --notes "Nightly builds from main branch." \
    --prerelease \
    --title "nightly" \
    --verify-tag \
    nightly \
    {{.Aarch64Darwin}}.tar.zst \
    {{.Aarch64Linux}}.tar.zst \
    {{.X8664Darwin}}.tar.zst \
    {{.X8664Linux}}.tar.zst`

func vorpal(context *config.ConfigContext) (*string, error) {
	protoc, err := artifact.Protoc(context)
	if err != nil {
		return nil, err
	}

	name := "vorpal"

	return language.NewRustBuilder(name).
		WithArtifacts([]*string{protoc}).
		WithBins([]string{name}).
		WithPackages([]string{
			"crates/agent",
			"crates/cli",
			"crates/registry",
			"crates/schema",
			"crates/sdk",
			"crates/store",
			"crates/worker",
		}).
		Build(context)
}

func vorpalProcess(context *config.ConfigContext) (*string, error) {
	vorpal, err := vorpal(context)
	if err != nil {
		return nil, err
	}

	entrypoint := fmt.Sprintf("%s/bin/vorpal", artifact.GetEnvKey(vorpal))

	return artifact.NewArtifactProcessBuilder("vorpal-process", entrypoint).
		WithArguments([]string{
			"--registry",
			"http://localhost:50051",
			"start",
			"--port",
			"50051",
		}).
		WithArtifacts([]*string{vorpal}).
		Build(context)
}

func vorpalRelease(context *config.ConfigContext) (*string, error) {
	varAarch64Darwin, err := artifact.
		NewArtifactVariableBuilder("aarch64-darwin").
		WithRequire().
		Build(context)
	if err != nil {
		return nil, err
	}

	varAarch64Linux, err := artifact.
		NewArtifactVariableBuilder("aarch64-linux").
		WithRequire().
		Build(context)
	if err != nil {
		return nil, err
	}

	varBranchName, err := artifact.
		NewArtifactVariableBuilder("branch-name").
		WithRequire().
		Build(context)
	if err != nil {
		return nil, err
	}

	varX8664Darwin, err := artifact.
		NewArtifactVariableBuilder("x8664-darwin").
		WithRequire().
		Build(context)
	if err != nil {
		return nil, err
	}

	varX8664Linux, err := artifact.
		NewArtifactVariableBuilder("x8664-linux").
		WithRequire().
		Build(context)
	if err != nil {
		return nil, err
	}

	aarch64Darwin, err := context.FetchArtifact(*varAarch64Darwin)
	if err != nil {
		return nil, err
	}

	aarch64Linux, err := context.FetchArtifact(*varAarch64Linux)
	if err != nil {
		return nil, err
	}

	gh, err := artifact.Gh(context)
	if err != nil {
		return nil, err
	}

	x8664Darwin, err := context.FetchArtifact(*varX8664Darwin)
	if err != nil {
		return nil, err
	}

	x8664Linux, err := context.FetchArtifact(*varX8664Linux)
	if err != nil {
		return nil, err
	}

	artifacts := []*string{
		aarch64Darwin,
		aarch64Linux,
		gh,
		x8664Darwin,
		x8664Linux,
	}

	scriptTemplate, err := template.New("script").Parse(releaseScript)
	if err != nil {
		return nil, err
	}

	var scriptBuffer bytes.Buffer

	scriptTemplateVars := releaseScriptArgs{
		Aarch64Darwin: *aarch64Darwin,
		Aarch64Linux:  *aarch64Linux,
		BranchName:    *varBranchName,
		X8664Darwin:   *x8664Darwin,
		X8664Linux:    *x8664Linux,
	}

	if err := scriptTemplate.Execute(&scriptBuffer, scriptTemplateVars); err != nil {
		return nil, err
	}

	return artifact.NewArtifactTaskBuilder("vorpal-release", scriptBuffer.String()).
		WithArtifacts(artifacts).
		Build(context)
}

func vorpalShell(context *config.ConfigContext) (*string, error) {
	gh, err := artifact.Gh(context)
	if err != nil {
		return nil, err
	}

	gobin, err := artifact.GoBin(context)
	if err != nil {
		return nil, err
	}

	goimports, err := artifact.Goimports(context)
	if err != nil {
		return nil, err
	}

	gopls, err := artifact.Gopls(context)
	if err != nil {
		return nil, err
	}

	grpcurl, err := artifact.Grpcurl(context)
	if err != nil {
		return nil, err
	}

	protoc, err := artifact.Protoc(context)
	if err != nil {
		return nil, err
	}

	protocGenGo, err := artifact.ProtocGenGo(context)
	if err != nil {
		return nil, err
	}

	protocGenGoGRPC, err := artifact.ProtocGenGoGRPC(context)
	if err != nil {
		return nil, err
	}

	staticcheck, err := artifact.Staticcheck(context)
	if err != nil {
		return nil, err
	}

	vorpalProcess, err := vorpalProcess(context)
	if err != nil {
		return nil, err
	}

	nginx, err := artifact.Nginx(context)
	if err != nil {
		return nil, err
	}

	return language.NewRustShellBuilder("vorpal-shell").
		WithArtifacts([]*string{
			gh,
			gobin,
			goimports,
			gopls,
			grpcurl,
			nginx,
			protoc,
			protocGenGo,
			protocGenGoGRPC,
			staticcheck,
			vorpalProcess,
		}).
		Build(context)
}

func vorpalTest(context *config.ConfigContext) (*string, error) {
	vorpal, err := vorpal(context)
	if err != nil {
		return nil, err
	}

	script := fmt.Sprintf("\n%s/bin/vorpal --version", artifact.GetEnvKey(vorpal))

	return artifact.NewArtifactTaskBuilder("vorpal-test", script).
		WithArtifacts([]*string{vorpal}).
		Build(context)
}

func main() {
	context := config.GetContext()
	contextArtifact := context.GetArtifactName()

	var err error

	switch contextArtifact {
	case "vorpal":
		_, err = vorpal(context)
	case "vorpal-process":
		_, err = vorpalProcess(context)
	case "vorpal-release":
		_, err = vorpalRelease(context)
	case "vorpal-shell":
		_, err = vorpalShell(context)
	case "vorpal-test":
		_, err = vorpalTest(context)
	default:
		log.Fatalf("unknown artifact %s", contextArtifact)
	}
	if err != nil {
		log.Fatalf("failed to build %s: %v", contextArtifact, err)
	}

	context.Run()
}
