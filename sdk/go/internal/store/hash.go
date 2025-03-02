package store

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io"
	"os"
)

// GetFileHash calculates the SHA-256 hash of a file
func GetFileHash(path string) (string, error) {
	// Check if path is a file
	fileInfo, err := os.Stat(path)
	if err != nil {
		return "", err
	}

	if fileInfo.IsDir() {
		return "", fmt.Errorf("path is not a file")
	}

	// Open the file
	file, err := os.Open(path)
	if err != nil {
		return "", err
	}
	defer file.Close()

	// Create a new SHA-256 hash
	hash := sha256.New()

	// Copy file content to hash
	if _, err := io.Copy(hash, file); err != nil {
		return "", fmt.Errorf("failed to get file hash: %w", err)
	}

	// Get the hash sum and convert to hex string
	hashSum := hash.Sum(nil)
	hashString := hex.EncodeToString(hashSum)

	return hashString, nil
}

// GetFileHashes calculates SHA-256 hashes for multiple files
func GetFileHashes(files []string) ([]string, error) {
	var hashes []string

	for _, file := range files {
		// Check if path is a file
		fileInfo, err := os.Stat(file)
		if err != nil {
			continue
		}

		if fileInfo.IsDir() {
			continue
		}

		hash, err := GetFileHash(file)
		if err != nil {
			return nil, err
		}

		hashes = append(hashes, hash)
	}

	return hashes, nil
}

// GetHashesDigest combines multiple hashes and creates a digest
func GetHashesDigest(hashes []string) (string, error) {
	combined := ""

	for _, hash := range hashes {
		combined += hash
	}

	// Create a new SHA-256 hash
	hash := sha256.New()

	// Write the combined string to the hash
	hash.Write([]byte(combined))

	// Get the hash sum and convert to hex string
	hashSum := hash.Sum(nil)
	hashString := hex.EncodeToString(hashSum)

	return hashString, nil
}

// HashFiles calculates a combined hash for multiple files
func HashFiles(paths []string) (string, error) {
	if len(paths) == 0 {
		return "", fmt.Errorf("no source files found")
	}

	pathsHashes, err := GetFileHashes(paths)
	if err != nil {
		return "", err
	}

	pathsHashesJoined, err := GetHashesDigest(pathsHashes)
	if err != nil {
		return "", err
	}

	return pathsHashesJoined, nil
}

// GetHashDigest calculates the SHA-256 hash of a string
func GetHashDigest(hash string) string {
	// Create a new SHA-256 hash
	hasher := sha256.New()

	// Write the string to the hash
	hasher.Write([]byte(hash))

	// Get the hash sum and convert to hex string
	hashSum := hasher.Sum(nil)
	hashString := hex.EncodeToString(hashSum)

	return hashString
}
