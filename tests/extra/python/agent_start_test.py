#!/usr/bin/env python3

# build: maturin develop --uv
# debugger: select "Python: Debug File" + F5
# script: /path/to/this/test/file.py


from orcapod import LocalDockerOrchestrator, Orchestrator, Agent, AgentClient
from asyncio import run

orch = LocalDockerOrchestrator()
# orch.__class__ = Orchestrator

# agent_client = AgentClient(group="test", host="alpha")

agent = Agent(group="test", host="alpha", orchestrator=orch, is_queryable=True)
agent_client = agent.client()

run(agent.start(namespace_lookup={}, store=None))
# agent.start(namespace_lookup={}, queryable=True, store=None)
