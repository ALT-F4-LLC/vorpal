package main

import (
	"log"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact/language"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func main() {
	ctx := config.GetContext()

	// protoc is included by default; WithoutProtoc() removes it
	_, err := language.NewGoDevelopmentEnvironment("my-project-dev", config.SYSTEMS).
		WithEnvironments([]string{"CGO_ENABLED=0"}).
		Build(ctx)
	if err != nil {
		log.Fatalf("error building environment: %v", err)
	}

	ctx.Run()
}
