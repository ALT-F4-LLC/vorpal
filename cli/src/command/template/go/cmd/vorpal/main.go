package main

import (
	"log"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact/language"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func main() {
	// Define build context

	ctx := config.GetContext()

	// Define supported artifact systems

	systems := []api.ArtifactSystem{
		api.ArtifactSystem_AARCH64_DARWIN,
		api.ArtifactSystem_AARCH64_LINUX,
		api.ArtifactSystem_X8664_DARWIN,
		api.ArtifactSystem_X8664_LINUX,
	}

	// Define language-specific development environment artifact

	_, err := language.NewGoDevelopmentEnvironment("example-shell", systems).
		Build(ctx)
	if err != nil {
		log.Fatalf("error building development environment: %v", err)
	}

	// Define application artifact 

	_, err = language.NewGo("example", systems).
		WithBuildDirectory("cmd/example").
		WithIncludes([]string{"cmd/example", "go.mod", "go.sum"}).
		Build(ctx)
	if err != nil {
		log.Fatalf("error building: %v", err)
	}

	// Run context to build

	ctx.Run()
}
