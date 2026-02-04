package main

import (
	"fmt"
	"log"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact/language"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

var Systems = []api.ArtifactSystem{
	api.ArtifactSystem_AARCH64_DARWIN,
	api.ArtifactSystem_AARCH64_LINUX,
	api.ArtifactSystem_X8664_DARWIN,
	api.ArtifactSystem_X8664_LINUX,
}

func main() {
	context := config.GetContext()
	contextTarget := context.GetTarget()

	// Artifact dependencies

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

	// Artifacts

	goarch, err := language.GetGOARCH(contextTarget)
	if err != nil {
		log.Fatalf("failed to get GOARCH for target %s: %v", contextTarget, err)
	}

	goos, err := language.GetGOOS(contextTarget)
	if err != nil {
		log.Fatalf("failed to get GOOS for target %s: %v", contextTarget, err)
	}

	_, err = artifact.
		NewProjectEnvironment("example-shell", Systems).
		WithArtifacts([]*string{
			gobin,
			goimports,
			gopls,
			protoc,
			protocGenGo,
			protocGenGoGRPC,
			staticcheck,
		}).
		WithEnvironments([]string{
			"CGO_ENABLED=0",
			fmt.Sprintf("GOARCH=%s", *goarch),
			fmt.Sprintf("GOOS=%s", *goos),
		}).
		Build(context)
	if err != nil {
		log.Fatalf("error building project environment: %v", err)
	}

	_, err = language.NewGo("example", Systems).
		WithBuildDirectory("cmd/example").
		WithIncludes([]string{
			"cmd/example",
			"go.mod",
			"go.sum",
		}).
		Build(context)
	if err != nil {
		log.Fatalf("error building: %v", err)
	}

	// Run the build

	context.Run()
}
