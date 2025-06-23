from orcapod import (
    Agent,
    AgentClient,
    LocalDockerOrchestrator,
    PodJob,
    OrcaPath,
    Pod,
    Annotation,
)
import asyncio
import zenoh

group = "test"
host = "alpha"

client = AgentClient(group=group, host=host)
agent = Agent(group=group, host=host, orchestrator=LocalDockerOrchestrator())

pod_job_count = 4
namespace_lookup = {
    "default": "tests/.tmp/default",
}
pod_jobs = [
    PodJob(
        annotation=Annotation(
            name="simple",
            description="This is an example pod job.",
            version=f"0.{i}.0",
        ),
        pod=Pod(
            annotation=None,
            image="ghcr.io/colinianking/stress-ng:e2f96874f951a72c1c83ff49098661f0e013ac40",
            command="stress-ng --cpu 1 --cpu-load 100 --timeout 5 --metrics-brief",
            input_stream={},
            output_dir="/tmp/output",
            output_stream={},
            source_commit_url="https://github.com/user/simple",
            recommended_cpus=0.1,
            recommended_memory=10 << 20,
            required_gpu=None,
        ),
        input_stream={},
        output_dir=OrcaPath(
            namespace="default",
            path=".",
        ),
        cpu_limit=1,
        memory_limit=10 << 20,
        env_vars=None,
        namespace_lookup=namespace_lookup,
    )
    for i in range(1, pod_job_count + 1)
]


async def verify():
    counter = 0

    def count(sample):
        nonlocal counter
        counter += 1

    with zenoh.open(zenoh.Config()) as session:
        with session.declare_subscriber(
            f"group/{group}/success/pod_job/**", count
        ) as subscriber:
            await asyncio.sleep(10)

    if counter != pod_job_count:
        raise Exception(f"Unexpected successful pod job count: {counter}.")


async def main():
    watcher = asyncio.create_task(client.watch(key_expr="**"))
    worker = asyncio.create_task(agent.start(namespace_lookup=namespace_lookup))
    await asyncio.sleep(5)  # ensure service ready

    await client.submit_pod_jobs(pod_jobs=pod_jobs)
    await verify()


asyncio.run(main())
