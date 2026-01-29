package store

import (
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"syscall"
	"time"

	"github.com/google/uuid"
)

func GetStoreDirName(hash string, name string) string {
	return fmt.Sprintf("%s-%s", name, hash)
}

func GetRootDirPath() string {
	return "/var/lib/vorpal"
}

func GetCacheDirPath() string {
	return fmt.Sprintf("%s/cache", GetRootDirPath())
}

func GetSandboxDirPath() string {
	return fmt.Sprintf("%s/sandbox", GetRootDirPath())
}

func GetCacheArchivePath(hash string, name string) string {
	return fmt.Sprintf("%s/%s.tar.zst", GetCacheDirPath(), GetStoreDirName(hash, name))
}

func GetSandboxPath() (*string, error) {
	id, err := uuid.NewV7()
	if err != nil {
		return nil, err
	}

	path := fmt.Sprintf("%s/%s", GetSandboxDirPath(), id.String())

	return &path, nil
}

func shouldExclude(path string, excludes []string) bool {
	for _, ex := range excludes {
		// NOTE: Here, you might use filepath.Match for glob-style patterns.
		if strings.Contains(path, ex) {
			return true
		}
	}

	return false
}

func shouldInclude(path string, includes []string) bool {
	// If no includes are provided, consider every file valid.
	if len(includes) == 0 {
		return true
	}

	for _, inc := range includes {
		// NOTE: Again, you might use a more sophisticated matching mechanism.
		if strings.Contains(path, inc) {
			return true
		}
	}

	return false
}

func GetFilePaths(inputPath string, excludes []string, includes []string) ([]string, error) {
	paths := []string{}

	err := filepath.WalkDir(inputPath, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}

		if shouldExclude(path, excludes) {
			// If it's a directory, skip the entire directory tree.
			if d.IsDir() {
				return filepath.SkipDir
			}

			// Otherwise, just skip this file.
			return nil
		}

		if d.IsDir() {
			return nil
		}

		if !shouldInclude(path, includes) {
			return nil
		}

		paths = append(paths, path)

		return nil
	})
	if err != nil {
		return nil, err
	}

	// Sort paths for reproducible build hashes
	sort.Strings(paths)

	return paths, nil
}

func CopyFiles(sourcePath string, sourcePathFiles []string, targetPath string) ([]string, error) {
	if len(sourcePathFiles) == 0 {
		return nil, fmt.Errorf("no source files found")
	}

	for _, src := range sourcePathFiles {
		if strings.HasSuffix(src, ".tar.zst") {
			return nil, fmt.Errorf("source file is a tar.zst archive")
		}

		if _, err := os.Stat(src); os.IsNotExist(err) {
			return nil, fmt.Errorf("source file not found: %s", src)
		}

		fileInfo, err := os.Lstat(src)
		if err != nil {
			return nil, fmt.Errorf("failed to read metadata: %w", err)
		}

		// Calculate destination path by removing source prefix
		relPath, err := filepath.Rel(sourcePath, src)
		if err != nil {
			return nil, fmt.Errorf("failed to get relative path: %w", err)
		}

		dest := filepath.Join(targetPath, relPath)

		if fileInfo.IsDir() {
			if err := os.MkdirAll(dest, 0o755); err != nil {
				return nil, fmt.Errorf("create directory fail: %w", err)
			}
		} else if fileInfo.Mode()&os.ModeSymlink != 0 {
			// Handle symlink
			linkTarget, err := os.Readlink(src)
			if err != nil {
				return nil, fmt.Errorf("failed to read symlink: %w", err)
			}

			// Ensure parent directory exists
			parent := filepath.Dir(dest)
			if err := os.MkdirAll(parent, 0o755); err != nil {
				return nil, fmt.Errorf("create parent directory fail: %w", err)
			}

			// Remove existing symlink if it exists
			if _, err := os.Lstat(dest); err == nil {
				os.Remove(dest)
			}

			if err := os.Symlink(linkTarget, dest); err != nil {
				return nil, fmt.Errorf("symlink file fail: %w", err)
			}
		} else if fileInfo.Mode().IsRegular() {
			// Handle regular file
			parent := filepath.Dir(dest)
			if err := os.MkdirAll(parent, 0o755); err != nil {
				return nil, fmt.Errorf("create parent directory fail: %w", err)
			}

			srcFile, err := os.Open(src)
			if err != nil {
				return nil, fmt.Errorf("open source file fail: %w", err)
			}

			defer srcFile.Close()

			destFile, err := os.Create(dest)
			if err != nil {
				return nil, fmt.Errorf("create destination file fail: %w", err)
			}

			defer destFile.Close()

			if _, err := io.Copy(destFile, srcFile); err != nil {
				return nil, fmt.Errorf("copy file fail: %w", err)
			}

			// Set the same permissions
			if err := os.Chmod(dest, fileInfo.Mode()); err != nil {
				return nil, fmt.Errorf("set file permissions fail: %w", err)
			}
		} else {
			return nil, fmt.Errorf("source file is not a file or directory: %s", src)
		}
	}

	// Get all files in the target directory
	targetPathFiles, err := GetFilePaths(targetPath, []string{}, []string{})
	if err != nil {
		return nil, err
	}

	return targetPathFiles, nil
}

// SetSymlinkTimestamps sets the access and modification times of a symlink
func SetSymlinkTimestamps(path string) error {
	// Create epoch time (January 1, 1970 UTC)
	epoch := time.Unix(0, 0)

	// Convert to syscall timespec
	ts := []syscall.Timespec{
		{Sec: epoch.Unix(), Nsec: 0}, // Access time
		{Sec: epoch.Unix(), Nsec: 0}, // Modification time
	}

	// Use Utimesnano syscall to set symlink timestamps without following the link
	return syscall.UtimesNano(path, ts)
}

// SetTimestamps sets the access and modification times of a file or symlink to epoch (0)
func SetTimestamps(path string) error {
	// Check if the path is a symlink
	fileInfo, err := os.Lstat(path)
	if err != nil {
		return err
	}

	// Set timestamps based on whether it's a symlink or regular file
	if fileInfo.Mode()&os.ModeSymlink != 0 {
		return SetSymlinkTimestamps(path)
	} else {
		// Create epoch time (January 1, 1970 UTC)
		epoch := time.Unix(0, 0)
		// Set access and modification times for regular files
		return os.Chtimes(path, epoch, epoch)
	}
}
