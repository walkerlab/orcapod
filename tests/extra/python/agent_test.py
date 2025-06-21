from orcapod import Agent, AgentClient, LocalDockerOrchestrator

group = "test"
host = "alpha"

client = AgentClient(group=group, host=host)
agent = Agent(group=group, host=host, orchestrator=LocalDockerOrchestrator())
