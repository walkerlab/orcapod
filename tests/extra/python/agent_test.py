#!/usr/bin/env python3

# build: maturin develop --uv
# debugger: select "Python: Debug agent test" + F5
# script: /path/to/this/test/file.py /path/to/directory (must exist, will create subdirectory)
import shutil
from pathlib import Path
import argparse
import asyncio
import zenoh
from orcapod import (
    Agent,
    AgentClient,
    LocalDockerOrchestrator,
    LocalFileStore,
    PodJob,
    Uri,
    Pod,
    Annotation,
)


async def verify(group, pod_job_count):
    counter = 0

    def count(sample):
        nonlocal counter
        counter += 1

    with zenoh.open(zenoh.Config()) as session:
        with session.declare_subscriber(
            f"group/{group}/success/pod_job/**", count
        ) as subscriber:
            await asyncio.sleep(20)

    if counter != pod_job_count:
        raise Exception(f"Unexpected successful pod job count: {counter}.")


async def main(client, agent, test_dir, namespace_lookup, pod_jobs):
    watcher = asyncio.create_task(client.watch(key_expr="**"))
    worker = asyncio.create_task(
        agent.start(
            namespace_lookup=namespace_lookup,
            available_store=LocalFileStore(directory=f"{test_dir}/store"),
        ),
    )
    await asyncio.sleep(5)  # ensure service ready

    try:
        await client.submit_pod_jobs(pod_jobs=pod_jobs)
        await verify(client.group(), len(pod_jobs))
    finally:
        shutil.rmtree(test_dir)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Basic tests for orcapod's Python API."
    )
    parser.add_argument("test_dir", help="Root directory for tests.")
    args = parser.parse_args()

    test_dir = f"{args.test_dir}/{Path(__file__).stem}"

    group = "test"
    host = "alpha"

    client = AgentClient(group=group, host=host)
    agent = Agent(group=group, host=host, orchestrator=LocalDockerOrchestrator())

    namespace_lookup = {
        "default": f"{test_dir}/default",
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
                input_spec={},
                output_dir="/tmp/output",
                output_spec={},
                source_commit_url="https://github.com/user/simple",
                recommended_cpus=0.1,
                recommended_memory=10 << 20,
                required_gpu=None,
            ),
            input_packet={},
            output_dir=Uri(
                namespace="default",
                path=".",
            ),
            cpu_limit=1,
            memory_limit=10 << 20,
            env_vars=None,
            namespace_lookup=namespace_lookup,
        )
        for i in range(1, 5)
    ]

    asyncio.run(main(client, agent, test_dir, namespace_lookup, pod_jobs))
