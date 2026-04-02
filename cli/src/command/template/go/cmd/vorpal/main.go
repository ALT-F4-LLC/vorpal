package main

import (
	"log"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact/language"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"

	// Register linux_vorpal builder for Shell() on Linux targets
	_ "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact/linux_vorpal"
)

var Systems = []api.ArtifactSystem{
	api.ArtifactSystem_AARCH64_DARWIN,
	api.ArtifactSystem_AARCH64_LINUX,
	api.ArtifactSystem_X8664_DARWIN,
	api.ArtifactSystem_X8664_LINUX,
}

func main() {
	context := config.GetContext()

	// Development environment

	_, err := language.NewGoDevelopmentEnvironment("example-shell", Systems).Build(context)
	if err != nil {
		log.Fatalf("error building development environment: %v", err)
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
