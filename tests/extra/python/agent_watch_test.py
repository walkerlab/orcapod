#!/usr/bin/env python3

# build: maturin develop --uv
# debugger: select "Python: Debug File" + F5
# script: /path/to/this/test/file.py


# from orcapod import LocalDockerOrchestrator
# from asyncio import run

# orch = LocalDockerOrchestrator()

# pods = run(orch.list())  # only works with #[uniffi::export(async_runtime = "tokio")]
# # print(pods)

# # agent_info = orch.new_agent_blocking()
# agent_info = run(orch.new_agent())
# print(agent_info)

# run(orch.write_topic(agent_info, "/subject", "Hi, Raphael!"))


# print("hi")


from orcapod import LocalDockerOrchestrator, Orchestrator, Agent, AgentClient
from asyncio import run

orch = LocalDockerOrchestrator()
# orch.__class__ = Orchestrator

pods = run(orch.list())  # only works with #[uniffi::export(async_runtime = "tokio")]
print(pods)

agent_client = AgentClient("test", "alpha")

# agent = Agent("test", "alpha", orch)
# agent_client = agent.client()

run(agent_client.watch_topic())


print("hi")


# python -c '__import__("asyncio").run(__import__("orcapod").AgentClient("test", "alpha").write_topic("/subject", "Hi, Raphael!"))'

# python -c '__import__("asyncio").run(__import__("orcapod").AgentClient("test", "alpha").log("banana", "Does this work?"))'
