package main

import (
	"bytes"
	"fmt"
	"log"
	"os"
	"text/template"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
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

var SYSTEMS = []api.ArtifactSystem{
	api.ArtifactSystem_AARCH64_DARWIN,
	api.ArtifactSystem_AARCH64_LINUX,
	api.ArtifactSystem_X8664_DARWIN,
	api.ArtifactSystem_X8664_LINUX,
}

func vorpal(context *config.ConfigContext) (*string, error) {
	name := "vorpal"

	return language.NewRustBuilder(name, SYSTEMS).
		WithBins([]string{name}).
		WithIncludes([]string{"cli", "sdk/rust"}).
		WithPackages([]string{"vorpal-cli", "vorpal-sdk"}).
		Build(context)
}

func vorpalProcess(context *config.ConfigContext) (*string, error) {
	vorpal, err := vorpal(context)
	if err != nil {
		return nil, err
	}

	entrypoint := fmt.Sprintf("%s/bin/vorpal", artifact.GetEnvKey(vorpal))

	return artifact.NewArtifactProcessBuilder("vorpal-process", entrypoint, SYSTEMS).
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
		NewArtifactArgumentBuilder("aarch64-darwin").
		WithRequire().
		Build(context)
	if err != nil {
		return nil, err
	}

	varAarch64Linux, err := artifact.
		NewArtifactArgumentBuilder("aarch64-linux").
		WithRequire().
		Build(context)
	if err != nil {
		return nil, err
	}

	varBranchName, err := artifact.
		NewArtifactArgumentBuilder("branch-name").
		WithRequire().
		Build(context)
	if err != nil {
		return nil, err
	}

	varX8664Darwin, err := artifact.
		NewArtifactArgumentBuilder("x8664-darwin").
		WithRequire().
		Build(context)
	if err != nil {
		return nil, err
	}

	varX8664Linux, err := artifact.
		NewArtifactArgumentBuilder("x8664-linux").
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

	return artifact.NewArtifactTaskBuilder("vorpal-release", scriptBuffer.String(), SYSTEMS).
		WithArtifacts(artifacts).
		Build(context)
}

func vorpalDevenv(context *config.ConfigContext) (*string, error) {
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

	artifacts := []*string{
		gobin,
		goimports,
		gopls,
		grpcurl,
		protoc,
		protocGenGo,
		protocGenGoGRPC,
		staticcheck,
	}

	contextTarget := context.GetTarget()

	goarch, err := language.GetGOARCH(contextTarget)
	if err != nil {
		return nil, err
	}

	goos, err := language.GetGOOS(contextTarget)
	if err != nil {
		return nil, err
	}

	environments := []string{
		"CGO_ENABLED=0",
		fmt.Sprintf("GOARCH=%s", *goarch),
		fmt.Sprintf("GOOS=%s", *goos),
		fmt.Sprintf("PATH=%s", rustToolchainPath),
	}

	return artifact.ScriptDevenv(context, artifacts, environments, "vorpal-devenv", nil, SYSTEMS)
}

func vorpalTest(context *config.ConfigContext) (*string, error) {
	vorpal, err := vorpal(context)
	if err != nil {
		return nil, err
	}

	script := fmt.Sprintf("\n%s/bin/vorpal --version", artifact.GetEnvKey(vorpal))

	return artifact.NewArtifactTaskBuilder("vorpal-test", script, SYSTEMS).
		WithArtifacts([]*string{vorpal}).
		Build(context)
}

func vorpalUserenv(context *config.ConfigContext) (*string, error) {
	vorpal, err := vorpal(context)
	if err != nil {
		return nil, err
	}

	homeDir, err := os.UserHomeDir()
	if err != nil {
		log.Fatal(err)
	}

	artifacts := []*string{vorpal}

	symlinks := map[string]string{}

	symlinks[fmt.Sprintf("/var/lib/vorpal/store/artifact/output/%s/bin/vorpal", *vorpal)] = fmt.Sprintf("%s/.vorpal/bin/vorpal", homeDir)

	return artifact.ScriptUserenv(
		context,
		artifacts,
		nil,
		"vorpal-userenv",
		symlinks,
		SYSTEMS,
	)
}

func main() {
	context := config.GetContext()
	contextArtifact := context.GetArtifactName()

	var err error

	switch contextArtifact {
	case "vorpal":
		_, err = vorpal(context)
	case "vorpal-devenv":
		_, err = vorpalDevenv(context)
	case "vorpal-process":
		_, err = vorpalProcess(context)
	case "vorpal-release":
		_, err = vorpalRelease(context)
	case "vorpal-test":
		_, err = vorpalTest(context)
	case "vorpal-userenv":
		_, err = vorpalUserenv(context)
	default:
		log.Fatalf("unknown artifact %s", contextArtifact)
	}
	if err != nil {
		log.Fatalf("failed to build %s: %v", contextArtifact, err)
	}

	context.Run()
}
