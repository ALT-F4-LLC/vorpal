package config

import (
	"context"
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net"

	agentApi "github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/agent"
	artifactApi "github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
)

type ArtifactSource struct {
	Excludes []string
	Hash     *string
	Includes []string
	Path     string
}

type ArtifactSourceKind string

const (
	ArtifactSourceKind_GIT     ArtifactSourceKind = "GIT"
	ArtifactSourceKind_HTTP    ArtifactSourceKind = "HTTP"
	ArtifactSourceKind_LOCAL   ArtifactSourceKind = "LOCAL"
	ArtifactSourceKind_UNKNOWN ArtifactSourceKind = "UNKNOWN"
)

type ConfigContext struct {
	agent    string
	port     int
	registry string
	store    map[string]*artifactApi.Artifact
	system   artifactApi.ArtifactSystem
}

type ArtifactServer struct {
	artifactApi.UnimplementedArtifactServiceServer

	artifacts []*string
	store     map[string]*artifactApi.Artifact
}

func NewArtifactServer(artifacts []*string, store map[string]*artifactApi.Artifact) *ArtifactServer {
	return &ArtifactServer{
		artifacts: artifacts,
		store:     store,
	}
}

func (s *ArtifactServer) GetArtifact(ctx context.Context, request *artifactApi.ArtifactRequest) (*artifactApi.Artifact, error) {
	if request.Digest == "" {
		return nil, fmt.Errorf("'digest' is required")
	}

	response := s.store[request.Digest]
	if response == nil {
		return nil, fmt.Errorf("artifact not found")
	}

	return response, nil
}

func (s *ArtifactServer) GetArtifacts(ctx context.Context, request *artifactApi.ArtifactsRequest) (*artifactApi.ArtifactsResponse, error) {
	digests := make([]string, 0)

	for _, digest := range s.artifacts {
		if digest != nil {
			digests = append(digests, *digest)
		}
	}

	response := &artifactApi.ArtifactsResponse{
		Digests: digests,
	}

	return response, nil
}

func (s *ArtifactServer) StoreArtifact(ctx context.Context, request *artifactApi.Artifact) (*artifactApi.ArtifactResponse, error) {
	return nil, fmt.Errorf("not implemented")
}

func GetContext() *ConfigContext {
	cmd, err := NewCommand()
	if err != nil {
		log.Fatal(err)
	}

	return &ConfigContext{
		agent:    cmd.Agent,
		port:     cmd.Port,
		registry: cmd.Registry,
		store:    make(map[string]*artifactApi.Artifact),
		system:   cmd.Target,
	}
}

func (c *ConfigContext) AddArtifact(artifact *artifactApi.Artifact) (*string, error) {
	// 1. Setup systems

	artifactJson, err := json.Marshal(artifact)
	if err != nil {
		return nil, err
	}

	artifactDigest := fmt.Sprintf("%x", sha256.Sum256(artifactJson))

	if _, ok := c.store[artifactDigest]; ok {
		return &artifactDigest, nil
	}

	// TODO: make this run in parallel

	clientConn, err := grpc.NewClient(c.agent, grpc.WithTransportCredentials(insecure.NewCredentials()))
	if err != nil {
		return nil, err
	}

	defer clientConn.Close()

	client := agentApi.NewAgentServiceClient(clientConn)

	clientResponse, err := client.PrepareArtifact(context.Background(), artifact)
	if err != nil {
		return nil, fmt.Errorf("error preparing artifact: %v", err)
	}

	for {
		response, err := clientResponse.Recv()
		if err == io.EOF {
			break
		}

		if err != nil {
			return nil, fmt.Errorf("error receiving response: %v", err)
		}

		if response.ArtifactOutput != nil {
			log.Printf("%s {} |> %v", artifact.Name, response.ArtifactOutput)
		}

		artifact = response.Artifact
		artifactDigest = *response.ArtifactDigest
	}

	if _, ok := c.store[artifactDigest]; !ok {
		c.store[artifactDigest] = artifact
	}

	return &artifactDigest, nil
}

func (c *ConfigContext) FetchArtifact(digest string) (*string, error) {
	if _, ok := c.store[digest]; ok {
		return &digest, nil
	}

	clientConn, err := grpc.NewClient(c.agent, grpc.WithTransportCredentials(insecure.NewCredentials()))
	if err != nil {
		return nil, err
	}

	defer clientConn.Close()

	client := artifactApi.NewArtifactServiceClient(clientConn)

	clientResponse, err := client.GetArtifact(context.Background(), &artifactApi.ArtifactRequest{Digest: digest})
	if err != nil {
		return nil, fmt.Errorf("error fetching artifact: %v", err)
	}

	artifactJson, err := json.Marshal(clientResponse)
	if err != nil {
		return nil, err
	}

	artifactDigest := fmt.Sprintf("%x", sha256.Sum256(artifactJson))

	if _, ok := c.store[artifactDigest]; !ok {
		c.store[artifactDigest] = clientResponse
	}

	return &artifactDigest, nil
}

func (c *ConfigContext) GetArtifact(digest string) *artifactApi.Artifact {
	return c.store[digest]
}

func (c *ConfigContext) GetTarget() artifactApi.ArtifactSystem {
	return c.system
}

func (c *ConfigContext) Run(artifacts []*string) error {
	listener, err := net.Listen("tcp", fmt.Sprintf("[::]:%d", c.port))
	if err != nil {
		log.Fatalf("failed to listen: %v", err)
	}

	var opts []grpc.ServerOption

	grpcServer := grpc.NewServer(opts...)

	artifactApi.RegisterArtifactServiceServer(grpcServer, NewArtifactServer(artifacts, c.store))

	log.Printf("artifact service: %d", c.port)

	err = grpcServer.Serve(listener)
	if err != nil {
		return err
	}

	return nil
}
