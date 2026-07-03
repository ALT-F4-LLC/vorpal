from google.protobuf.internal import containers as _containers
from google.protobuf.internal import enum_type_wrapper as _enum_type_wrapper
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class ArtifactSystem(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    UNKNOWN_SYSTEM: _ClassVar[ArtifactSystem]
    AARCH64_DARWIN: _ClassVar[ArtifactSystem]
    AARCH64_LINUX: _ClassVar[ArtifactSystem]
    X8664_DARWIN: _ClassVar[ArtifactSystem]
    X8664_LINUX: _ClassVar[ArtifactSystem]
UNKNOWN_SYSTEM: ArtifactSystem
AARCH64_DARWIN: ArtifactSystem
AARCH64_LINUX: ArtifactSystem
X8664_DARWIN: ArtifactSystem
X8664_LINUX: ArtifactSystem

class ArtifactSource(_message.Message):
    __slots__ = ("digest", "excludes", "includes", "name", "path")
    DIGEST_FIELD_NUMBER: _ClassVar[int]
    EXCLUDES_FIELD_NUMBER: _ClassVar[int]
    INCLUDES_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    PATH_FIELD_NUMBER: _ClassVar[int]
    digest: str
    excludes: _containers.RepeatedScalarFieldContainer[str]
    includes: _containers.RepeatedScalarFieldContainer[str]
    name: str
    path: str
    def __init__(self, digest: _Optional[str] = ..., excludes: _Optional[_Iterable[str]] = ..., includes: _Optional[_Iterable[str]] = ..., name: _Optional[str] = ..., path: _Optional[str] = ...) -> None: ...

class ArtifactStepSecret(_message.Message):
    __slots__ = ("name", "value")
    NAME_FIELD_NUMBER: _ClassVar[int]
    VALUE_FIELD_NUMBER: _ClassVar[int]
    name: str
    value: str
    def __init__(self, name: _Optional[str] = ..., value: _Optional[str] = ...) -> None: ...

class ArtifactStep(_message.Message):
    __slots__ = ("entrypoint", "script", "secrets", "arguments", "artifacts", "environments")
    ENTRYPOINT_FIELD_NUMBER: _ClassVar[int]
    SCRIPT_FIELD_NUMBER: _ClassVar[int]
    SECRETS_FIELD_NUMBER: _ClassVar[int]
    ARGUMENTS_FIELD_NUMBER: _ClassVar[int]
    ARTIFACTS_FIELD_NUMBER: _ClassVar[int]
    ENVIRONMENTS_FIELD_NUMBER: _ClassVar[int]
    entrypoint: str
    script: str
    secrets: _containers.RepeatedCompositeFieldContainer[ArtifactStepSecret]
    arguments: _containers.RepeatedScalarFieldContainer[str]
    artifacts: _containers.RepeatedScalarFieldContainer[str]
    environments: _containers.RepeatedScalarFieldContainer[str]
    def __init__(self, entrypoint: _Optional[str] = ..., script: _Optional[str] = ..., secrets: _Optional[_Iterable[_Union[ArtifactStepSecret, _Mapping]]] = ..., arguments: _Optional[_Iterable[str]] = ..., artifacts: _Optional[_Iterable[str]] = ..., environments: _Optional[_Iterable[str]] = ...) -> None: ...

class Artifact(_message.Message):
    __slots__ = ("target", "sources", "steps", "systems", "aliases", "name")
    TARGET_FIELD_NUMBER: _ClassVar[int]
    SOURCES_FIELD_NUMBER: _ClassVar[int]
    STEPS_FIELD_NUMBER: _ClassVar[int]
    SYSTEMS_FIELD_NUMBER: _ClassVar[int]
    ALIASES_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    target: ArtifactSystem
    sources: _containers.RepeatedCompositeFieldContainer[ArtifactSource]
    steps: _containers.RepeatedCompositeFieldContainer[ArtifactStep]
    systems: _containers.RepeatedScalarFieldContainer[ArtifactSystem]
    aliases: _containers.RepeatedScalarFieldContainer[str]
    name: str
    def __init__(self, target: _Optional[_Union[ArtifactSystem, str]] = ..., sources: _Optional[_Iterable[_Union[ArtifactSource, _Mapping]]] = ..., steps: _Optional[_Iterable[_Union[ArtifactStep, _Mapping]]] = ..., systems: _Optional[_Iterable[_Union[ArtifactSystem, str]]] = ..., aliases: _Optional[_Iterable[str]] = ..., name: _Optional[str] = ...) -> None: ...

class ArtifactRequest(_message.Message):
    __slots__ = ("digest", "namespace")
    DIGEST_FIELD_NUMBER: _ClassVar[int]
    NAMESPACE_FIELD_NUMBER: _ClassVar[int]
    digest: str
    namespace: str
    def __init__(self, digest: _Optional[str] = ..., namespace: _Optional[str] = ...) -> None: ...

class ArtifactResponse(_message.Message):
    __slots__ = ("digest",)
    DIGEST_FIELD_NUMBER: _ClassVar[int]
    digest: str
    def __init__(self, digest: _Optional[str] = ...) -> None: ...

class ArtifactsRequest(_message.Message):
    __slots__ = ("digests", "namespace")
    DIGESTS_FIELD_NUMBER: _ClassVar[int]
    NAMESPACE_FIELD_NUMBER: _ClassVar[int]
    digests: _containers.RepeatedScalarFieldContainer[str]
    namespace: str
    def __init__(self, digests: _Optional[_Iterable[str]] = ..., namespace: _Optional[str] = ...) -> None: ...

class ArtifactsResponse(_message.Message):
    __slots__ = ("digests",)
    DIGESTS_FIELD_NUMBER: _ClassVar[int]
    digests: _containers.RepeatedScalarFieldContainer[str]
    def __init__(self, digests: _Optional[_Iterable[str]] = ...) -> None: ...

class GetArtifactAliasRequest(_message.Message):
    __slots__ = ("system", "name", "namespace", "tag")
    SYSTEM_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    NAMESPACE_FIELD_NUMBER: _ClassVar[int]
    TAG_FIELD_NUMBER: _ClassVar[int]
    system: ArtifactSystem
    name: str
    namespace: str
    tag: str
    def __init__(self, system: _Optional[_Union[ArtifactSystem, str]] = ..., name: _Optional[str] = ..., namespace: _Optional[str] = ..., tag: _Optional[str] = ...) -> None: ...

class GetArtifactAliasResponse(_message.Message):
    __slots__ = ("digest",)
    DIGEST_FIELD_NUMBER: _ClassVar[int]
    digest: str
    def __init__(self, digest: _Optional[str] = ...) -> None: ...

class StoreArtifactRequest(_message.Message):
    __slots__ = ("artifact", "artifact_aliases", "artifact_namespace")
    ARTIFACT_FIELD_NUMBER: _ClassVar[int]
    ARTIFACT_ALIASES_FIELD_NUMBER: _ClassVar[int]
    ARTIFACT_NAMESPACE_FIELD_NUMBER: _ClassVar[int]
    artifact: Artifact
    artifact_aliases: _containers.RepeatedScalarFieldContainer[str]
    artifact_namespace: str
    def __init__(self, artifact: _Optional[_Union[Artifact, _Mapping]] = ..., artifact_aliases: _Optional[_Iterable[str]] = ..., artifact_namespace: _Optional[str] = ...) -> None: ...
