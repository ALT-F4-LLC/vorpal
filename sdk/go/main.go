package main

import (
	"bytes"
	"log"
	"text/template"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/artifact/language"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

type ReleaseScriptTemplateArgs struct {
	Aarch64Darwin string
	Aarch64Linux  string
	BranchName    string
	X8664Darwin   string
	X8664Linux    string
}

const ReleaseScriptTemplate = `
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
    /var/lib/vorpal/store/{{.Aarch64Darwin}}.tar.zst`

func main() {
	context := config.GetContext()

	// Artifacts

	gh, err := artifact.Gh(context)
	if err != nil {
		log.Fatalf("failed to get gh artifact: %v", err)
	}

	gobin, err := artifact.GoBin(context)
	if err != nil {
		log.Fatalf("failed to get gobin artifact: %v", err)
	}

	goimports, err := artifact.Goimports(context)
	if err != nil {
		log.Fatalf("failed to get goimports artifact: %v", err)
	}

	gopls, err := artifact.Gopls(context)
	if err != nil {
		log.Fatalf("failed to get gopls artifact: %v", err)
	}

	grpcurl, err := artifact.Grpcurl(context)
	if err != nil {
		log.Fatalf("failed to get grpcurl artifact: %v", err)
	}

	protoc, err := artifact.Protoc(context)
	if err != nil {
		log.Fatalf("failed to get protoc artifact: %v", err)
	}

	protocGenGo, err := artifact.ProtocGenGo(context)
	if err != nil {
		log.Fatalf("failed to get protoc-gen-go artifact: %v", err)
	}

	protocGenGoGRPC, err := artifact.ProtocGenGoGRPC(context)
	if err != nil {
		log.Fatalf("failed to get protoc-gen-go-grpc artifact: %v", err)
	}

	staticcheck, err := artifact.Staticcheck(context)
	if err != nil {
		log.Fatalf("failed to get staticcheck artifact: %v", err)
	}

	_, err = language.NewRustShellBuilder("vorpal-shell").
		WithArtifacts([]*string{
			gh,
			gobin,
			goimports,
			gopls,
			grpcurl,
			protoc,
			protocGenGo,
			protocGenGoGRPC,
			staticcheck,
		}).
		Build(context)
	if err != nil {
		log.Fatalf("failed to create vorpal shell artifact: %v", err)
	}

	vorpal, err := language.NewRustBuilder("vorpal").
		WithArtifacts([]*string{
			protoc,
		}).
		WithBins([]string{
			"vorpal",
		}).
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
	if err != nil {
		log.Fatalf("failed to create vorpal artifact: %v", err)
	}

	// Tasks

	switch context.GetArtifactName() {
	case "vorpal-example":
		_, err = artifact.NewArtifactTaskBuilder("vorpal-example", "\nvorpal --version").
			WithArtifacts([]*string{
				vorpal,
			}).
			Build(context)
		if err != nil {
			log.Fatalf("failed to create vorpal-example artifact: %v", err)
		}

	case "vorpal-release":
		varAarch64Darwin, err := artifact.
			NewVariableBuilder("aarch64-darwin").
			WithRequire().
			Build(context)
		if err != nil {
			log.Fatalf("failed to get aarch64-darwin artifact: %v", err)
		}

		varAarch64Linux, err := artifact.
			NewVariableBuilder("aarch64-linux").
			WithRequire().
			Build(context)
		if err != nil {
			log.Fatalf("failed to get aarch64-linux artifact: %v", err)
		}

		varBranchName, err := artifact.
			NewVariableBuilder("branch-name").
			WithRequire().
			Build(context)
		if err != nil {
			log.Fatalf("failed to get branch-name artifact: %v", err)
		}

		varX8664Darwin, err := artifact.
			NewVariableBuilder("x8664-darwin").
			WithRequire().
			Build(context)
		if err != nil {
			log.Fatalf("failed to get x8664-darwin artifact: %v", err)
		}

		varX8664Linux, err := artifact.
			NewVariableBuilder("x8664-linux").
			WithRequire().
			Build(context)
		if err != nil {
			log.Fatalf("failed to get x8664-linux artifact: %v", err)
		}

		aarch64Darwin, err := context.FetchArtifact(*varAarch64Darwin)
		if err != nil {
			log.Fatalf("failed to fetch aarch64-darwin artifact: %v", err)
		}

		aarch64Linux, err := context.FetchArtifact(*varAarch64Linux)
		if err != nil {
			log.Fatalf("failed to fetch aarch64-linux artifact: %v", err)
		}

		x8664Darwin, err := context.FetchArtifact(*varX8664Darwin)
		if err != nil {
			log.Fatalf("failed to fetch x8664-darwin artifact: %v", err)
		}

		x8664Linux, err := context.FetchArtifact(*varX8664Linux)
		if err != nil {
			log.Fatalf("failed to fetch x8664-linux artifact: %v", err)
		}

		artifacts := []*string{
			aarch64Darwin,
			aarch64Linux,
			gh,
			x8664Darwin,
			x8664Linux,
		}

		scriptTemplate, err := template.New("script").Parse(ReleaseScriptTemplate)
		if err != nil {
			log.Fatalf("failed to parse script template: %v", err)
		}

		var scriptBuffer bytes.Buffer

		scriptTemplateVars := ReleaseScriptTemplateArgs{
			Aarch64Darwin: *aarch64Darwin,
			Aarch64Linux:  *aarch64Linux,
			BranchName:    *varBranchName,
			X8664Darwin:   *x8664Darwin,
			X8664Linux:    *x8664Linux,
		}

		if err := scriptTemplate.Execute(&scriptBuffer, scriptTemplateVars); err != nil {
			log.Fatalf("failed to execute script template: %v", err)
		}

		_, err = artifact.NewArtifactTaskBuilder("vorpal-release", scriptBuffer.String()).
			WithArtifacts(artifacts).
			Build(context)
		if err != nil {
			log.Fatalf("failed to create vorpal-release artifact: %v", err)
		}
	}

	context.Run()
}
