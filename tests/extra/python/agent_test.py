from orcapod import Agent, LocalDockerOrchestrator

agent = Agent(group="test", host="alpha", orchestrator=LocalDockerOrchestrator())
