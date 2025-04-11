package main

import (
	"log"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/artifact/language"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

func main() {
	context := config.GetContext()

	vorpalShellArtifacts, err := newShellArtifacts(context)
	if err != nil {
		log.Fatalf("failed to create shell artifacts: %v", err)
	}

	vorpalShell, err := language.NewRustShellBuilder("vorpal-shell").
		WithArtifacts(vorpalShellArtifacts).
		Build(context)
	if err != nil {
		log.Fatalf("failed to build shell: %v", err)
	}

	vorpalArtifacts, err := newArtifacts(context)
	if err != nil {
		log.Fatalf("failed to create artifacts: %v", err)
	}

	vorpal, err := language.NewRustBuilder("vorpal").
		WithArtifacts(vorpalArtifacts).
		WithExcludes([]string{
			".env",
			".envrc",
			".github",
			".gitignore",
			".packer",
			".vagrant",
			"Dockerfile",
			"Vagrantfile",
			"dist",
			"makefile",
			"script",
			"sdk/go",
			"shell.nix",
			"vorpal-config",
			"vorpal-domains.svg",
			"vorpal-purpose.jpg",
		}).
		Build(context)
	if err != nil {
		log.Fatalf("failed to build artifacts: %v", err)
	}

	context.Run([]*string{vorpalShell, vorpal})
}
