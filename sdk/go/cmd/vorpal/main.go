package main

import (
	"log"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/cmd/vorpal/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func main() {
	context := config.GetContext()

	var err error
	switch context.GetArtifactName() {
	case "vorpal":
		_, err = artifact.Vorpal(context)
	case "vorpal-container-image":
		_, err = artifact.BuildVorpalContainerImage(context)
	case "vorpal-job":
		_, err = artifact.BuildVorpalJob(context)
	case "vorpal-process":
		_, err = artifact.BuildVorpalProcess(context)
	case "vorpal-release":
		_, err = artifact.BuildVorpalRelease(context)
	case "vorpal-shell":
		_, err = artifact.BuildVorpalShell(context)
	case "vorpal-user":
		_, err = artifact.BuildVorpalUser(context)
	}

	if err != nil {
		log.Fatalf("failed to build artifact: %v", err)
	}

	context.Run()
}
