package main

import (
	"log"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact/language"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func main() {
	ctx := config.GetContext()

	systems := []string{
		"aarch64-darwin",
		"aarch64-linux",
		"x86_64-darwin",
		"x86_64-linux",
	}

	_, err := language.NewGoDevelopmentEnvironment("example-shell", systems).
		Build(ctx)
	if err != nil {
		log.Fatalf("error building development environment: %v", err)
	}

	_, err = language.NewGo("example", systems).
		WithBuildDirectory("cmd/example").
		WithIncludes([]string{"cmd/example", "go.mod", "go.sum"}).
		Build(ctx)
	if err != nil {
		log.Fatalf("error building: %v", err)
	}

	ctx.Run()
}
