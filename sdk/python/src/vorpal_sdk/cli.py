"""CLI argument parsing for the Vorpal SDK ``start`` subcommand.

Mirrors ``sdk/typescript/src/cli.ts`` and the Rust SDK's clap structure:

    start --agent URL --artifact NAME --artifact-context PATH
          --artifact-namespace NS --artifact-system SYS
          --port PORT --registry URL
          [--artifact-unlock] [--artifact-variable KEY=VALUE ...]

These arguments are supplied automatically by the Vorpal CLI when invoking a
compiled config binary; you typically do not construct a ``StartCommand``
manually — use :meth:`vorpal_sdk.context.ConfigContext.create` instead.
"""

from __future__ import annotations

import os
from dataclasses import dataclass, field


@dataclass
class StartCommand:
    """Parsed result of the ``start`` CLI subcommand."""

    agent: str
    artifact: str
    artifact_context: str
    artifact_namespace: str
    artifact_system: str
    port: int
    registry: str
    artifact_unlock: bool = False
    artifact_variable: list[str] = field(default_factory=list)


def _get_default_address() -> str:
    env_socket = os.environ.get("VORPAL_SOCKET_PATH")
    if env_socket and env_socket.strip() != "":
        return f"unix://{env_socket}"
    return "unix:///var/lib/vorpal/vorpal.sock"


def parse_cli_args(argv: list[str] | None = None) -> StartCommand:
    """Parse ``start`` CLI arguments matching the Rust/TS SDK structure.

    Hand-rolled (not ``argparse``) to preserve byte-for-byte behavioral parity
    with ``cli.ts``: the same flag set, the same ``--artifact-variable`` repeat
    semantics, the same required-field errors, and the same unknown-argument
    rejection. ``argparse``'s prefix-matching, abbreviation, and ``-h`` handling
    would diverge from the reference.
    """
    import sys

    if argv is None:
        argv = sys.argv[1:]

    if len(argv) == 0 or argv[0] != "start":
        raise ValueError("expected 'start' subcommand")

    args = argv[1:]
    default_address = _get_default_address()

    agent = default_address
    artifact = ""
    artifact_context = ""
    artifact_namespace = ""
    artifact_system = ""
    artifact_unlock = False
    artifact_variable: list[str] = []
    port = 0
    port_provided = False
    registry = default_address

    i = 0
    while i < len(args):
        arg = args[i]

        def consume_value(flag: str) -> str:
            nonlocal i
            i += 1
            if i >= len(args):
                raise ValueError(f"{flag} requires a value")
            return args[i]

        if arg == "--agent":
            agent = consume_value(arg)
        elif arg == "--artifact":
            artifact = consume_value(arg)
        elif arg == "--artifact-context":
            artifact_context = consume_value(arg)
        elif arg == "--artifact-namespace":
            artifact_namespace = consume_value(arg)
        elif arg == "--artifact-system":
            artifact_system = consume_value(arg)
        elif arg == "--artifact-unlock":
            artifact_unlock = True
        elif arg == "--artifact-variable":
            artifact_variable.append(consume_value(arg))
        elif arg == "--port":
            port_str = consume_value(arg)
            try:
                port = int(port_str)
            except ValueError:
                raise ValueError(
                    f"--port value is not a valid number: {port_str}"
                ) from None
            port_provided = True
        elif arg == "--registry":
            registry = consume_value(arg)
        else:
            raise ValueError(f"unknown argument: {arg}")

        i += 1

    if not artifact:
        raise ValueError("--artifact is required")
    if not artifact_context:
        raise ValueError("--artifact-context is required")
    if not artifact_namespace:
        raise ValueError("--artifact-namespace is required")
    if not artifact_system:
        raise ValueError("--artifact-system is required")
    if not port_provided:
        raise ValueError("--port is required")

    return StartCommand(
        agent=agent,
        artifact=artifact,
        artifact_context=artifact_context,
        artifact_namespace=artifact_namespace,
        artifact_system=artifact_system,
        port=port,
        registry=registry,
        artifact_unlock=artifact_unlock,
        artifact_variable=artifact_variable,
    )
