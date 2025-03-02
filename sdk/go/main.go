package main

import (
	"log"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/context"
)

func main() {
	ctx := context.GetContext()

	log.Printf("Context: %v", ctx)
}
