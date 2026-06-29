from vorpal_sdk.api.artifact import artifact_pb2 as _artifact_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class BuildArtifactRequest(_message.Message):
    __slots__ = ("artifact_aliases", "artifact_namespace", "registry", "artifact")
    ARTIFACT_ALIASES_FIELD_NUMBER: _ClassVar[int]
    ARTIFACT_NAMESPACE_FIELD_NUMBER: _ClassVar[int]
    REGISTRY_FIELD_NUMBER: _ClassVar[int]
    ARTIFACT_FIELD_NUMBER: _ClassVar[int]
    artifact_aliases: _containers.RepeatedScalarFieldContainer[str]
    artifact_namespace: str
    registry: str
    artifact: _artifact_pb2.Artifact
    def __init__(self, artifact_aliases: _Optional[_Iterable[str]] = ..., artifact_namespace: _Optional[str] = ..., registry: _Optional[str] = ..., artifact: _Optional[_Union[_artifact_pb2.Artifact, _Mapping]] = ...) -> None: ...

class BuildArtifactResponse(_message.Message):
    __slots__ = ("output",)
    OUTPUT_FIELD_NUMBER: _ClassVar[int]
    output: str
    def __init__(self, output: _Optional[str] = ...) -> None: ...
