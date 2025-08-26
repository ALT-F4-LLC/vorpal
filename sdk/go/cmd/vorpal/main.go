package main

import (
	"bytes"
	"fmt"
	"log"
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

func main() {
	context := config.GetContext()
	contextTarget := context.GetTarget()

	// Dependencies

	gobin, err := artifact.GoBin(context)
	if err != nil {
		log.Fatalf("failed to get go: %v", err)
	}

	goimports, err := artifact.Goimports(context)
	if err != nil {
		log.Fatalf("failed to get goimports: %v", err)
	}

	gopls, err := artifact.Gopls(context)
	if err != nil {
		log.Fatalf("failed to get gopls: %v", err)
	}

	grpcurl, err := artifact.Grpcurl(context)
	if err != nil {
		log.Fatalf("failed to get grpcurl: %v", err)
	}

	protoc, err := artifact.Protoc(context)
	if err != nil {
		log.Fatalf("failed to get protoc: %v", err)
	}

	protocGenGo, err := artifact.ProtocGenGo(context)
	if err != nil {
		log.Fatalf("failed to get protoc-gen-go: %v", err)
	}

	protocGenGoGRPC, err := artifact.ProtocGenGoGRPC(context)
	if err != nil {
		log.Fatalf("failed to get protoc-gen-go-grpc: %v", err)
	}

	staticcheck, err := artifact.Staticcheck(context)
	if err != nil {
		log.Fatalf("failed to get staticcheck: %v", err)
	}

	// Vorpal

	vorpal, err := language.NewRustBuilder("vorpal", SYSTEMS).
		WithBins([]string{"vorpal"}).
		WithIncludes([]string{"cli", "sdk/rust"}).
		WithPackages([]string{"vorpal-cli", "vorpal-sdk"}).
		Build(context)
	if err != nil {
		log.Fatalf("failed to build vorpal: %v", err)
	}

	// Vorpal devenv

	goarch, err := language.GetGOARCH(contextTarget)
	if err != nil {
		log.Fatalf("failed to get GOARCH for target %s: %v", contextTarget, err)
	}

	goos, err := language.GetGOOS(contextTarget)
	if err != nil {
		log.Fatalf("failed to get GOOS for target %s: %v", contextTarget, err)
	}

	_, errDevenv := artifact.ScriptDevenv(
		context,
		[]*string{
			gobin,
			goimports,
			gopls,
			grpcurl,
			protoc,
			protocGenGo,
			protocGenGoGRPC,
			staticcheck,
		},
		[]string{
			"CGO_ENABLED=0",
			fmt.Sprintf("GOARCH=%s", *goarch),
			fmt.Sprintf("GOOS=%s", *goos),
		},
		"vorpal-devenv",
		nil,
		SYSTEMS,
	)
	if errDevenv != nil {
		log.Fatalf("failed to build vorpal-devenv: %v", errDevenv)
	}

	// Vorpal process

	_, errProcess := artifact.NewArtifactProcessBuilder(
		"vorpal-process",
		fmt.Sprintf("%s/bin/vorpal", artifact.GetEnvKey(vorpal)),
		SYSTEMS,
	).
		WithArguments([]string{
			"--registry",
			"https://localhost:50051",
			"start",
			"--port",
			"50051",
		}).
		WithArtifacts([]*string{vorpal}).
		Build(context)
	if errProcess != nil {
		log.Fatalf("failed to build vorpal-process: %v", errProcess)
	}

	// Vorpal task

	_, errTest := artifact.NewArtifactTaskBuilder(
		"vorpal-test",
		fmt.Sprintf("\n%s/bin/vorpal --version", artifact.GetEnvKey(vorpal)),
		SYSTEMS,
	).
		WithArtifacts([]*string{vorpal}).
		Build(context)
	if errTest != nil {
		log.Fatalf("failed to build vorpal-test: %v", errTest)
	}

	// Vorpal userenv

	_, errUserenv := artifact.ScriptUserenv(
		context,
		[]*string{},
		[]string{"PATH=$HOME/.vorpal/bin"},
		"vorpal-userenv",
		map[string]string{
			"$HOME/Development/repository/github.com/ALT-F4-LLC/vorpal.git/main/target/debug/vorpal": "$HOME/.vorpal/bin/vorpal",
		},
		SYSTEMS,
	)
	if errUserenv != nil {
		log.Fatalf("failed to build vorpal-userenv: %v", errUserenv)
	}

	if context.GetArtifactName() == "vorpal-release" {
		varAarch64Darwin, err := artifact.
			NewArtifactArgumentBuilder("aarch64-darwin").
			WithRequire().
			Build(context)
		if err != nil {
			log.Fatalf("failed to build artifact argument: %v", err)
		}

		varAarch64Linux, err := artifact.
			NewArtifactArgumentBuilder("aarch64-linux").
			WithRequire().
			Build(context)
		if err != nil {
			log.Fatalf("failed to build artifact argument: %v", err)
		}

		varBranchName, err := artifact.
			NewArtifactArgumentBuilder("branch-name").
			WithRequire().
			Build(context)
		if err != nil {
			log.Fatalf("failed to build artifact argument: %v", err)
		}

		varX8664Darwin, err := artifact.
			NewArtifactArgumentBuilder("x8664-darwin").
			WithRequire().
			Build(context)
		if err != nil {
			log.Fatalf("failed to build artifact argument: %v", err)
		}

		varX8664Linux, err := artifact.
			NewArtifactArgumentBuilder("x8664-linux").
			WithRequire().
			Build(context)
		if err != nil {
			log.Fatalf("failed to build artifact argument: %v", err)
		}

		aarch64Darwin, err := context.FetchArtifact(*varAarch64Darwin)
		if err != nil {
			log.Fatalf("failed to fetch artifact: %v", err)
		}

		aarch64Linux, err := context.FetchArtifact(*varAarch64Linux)
		if err != nil {
			log.Fatalf("failed to fetch artifact: %v", err)
		}

		githubCli, err := artifact.Gh(context)
		if err != nil {
			log.Fatalf("failed to get gh: %v", err)
		}

		x8664Darwin, err := context.FetchArtifact(*varX8664Darwin)
		if err != nil {
			log.Fatalf("failed to fetch artifact: %v", err)
		}

		x8664Linux, err := context.FetchArtifact(*varX8664Linux)
		if err != nil {
			log.Fatalf("failed to fetch artifact: %v", err)
		}

		scriptTemplate, err := template.New("script").Parse(releaseScript)
		if err != nil {
			log.Fatalf("failed to parse script template: %v", err)
		}

		var script bytes.Buffer

		scriptVars := releaseScriptArgs{
			Aarch64Darwin: *aarch64Darwin,
			Aarch64Linux:  *aarch64Linux,
			BranchName:    *varBranchName,
			X8664Darwin:   *x8664Darwin,
			X8664Linux:    *x8664Linux,
		}

		if scriptErr := scriptTemplate.Execute(&script, scriptVars); scriptErr != nil {
			log.Fatalf("failed to execute script template: %v", scriptErr)
		}

		_, errRelease := artifact.NewArtifactTaskBuilder("vorpal-release", script.String(), SYSTEMS).
			WithArtifacts([]*string{
				aarch64Darwin,
				aarch64Linux,
				githubCli,
				x8664Darwin,
				x8664Linux,
			}).
			Build(context)
		if errRelease != nil {
			log.Fatalf("failed to build vorpal-release: %v", errRelease)
		}
	}

	context.Run()
}
