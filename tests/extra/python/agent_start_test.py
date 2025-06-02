#!/usr/bin/env python3

# build: maturin develop --uv
# debugger: select "Python: Debug File" + F5
# script: /path/to/this/test/file.py


from orcapod import LocalDockerOrchestrator, Orchestrator, Agent, AgentClient
from asyncio import run

orch = LocalDockerOrchestrator()
# orch.__class__ = Orchestrator

# agent_client = AgentClient("test", "alpha")

agent = Agent("test", "alpha", orch)
agent_client = agent.client()

run(agent.start(namespace_lookup={}, queryable=True, store=None))
