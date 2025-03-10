package store

import (
	"fmt"
	"os"
)

func NewSandboxDir() (*string, error) {
	path, err := GetSandboxPath()
	if err != nil {
		return nil, err
	}

	if err := os.MkdirAll(*path, 0o755); err != nil {
		return nil, err
	}

	return path, nil
}

func NewSandboxFile(extension string) (*os.File, error) {
	path, err := GetSandboxPath()
	if err != nil {
		return nil, err
	}

	filename := fmt.Sprintf("%s%s", *path, extension)

	file, err := os.Create(filename)
	if err != nil {
		return nil, err
	}

	return file, nil
}
