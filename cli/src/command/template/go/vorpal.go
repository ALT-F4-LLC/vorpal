package main

import (
	"log"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact/language"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func main() {
	context := config.GetContext()

	systems := []api.ArtifactSystem{
		api.ArtifactSystem_AARCH64_DARWIN,
		api.ArtifactSystem_AARCH64_LINUX,
		api.ArtifactSystem_X8664_DARWIN,
		api.ArtifactSystem_X8664_LINUX,
	}

	_, err := language.NewGoBuilder("example", systems).
		WithIncludes([]string{"main.go", "go.mod", "go.sum"}).
		Build(context)
	if err != nil {
		log.Fatalf("error building: %v", err)
	}

	context.Run()
}
