#!/usr/bin/env python3

# build: maturin develop --uv
# debugger: select "Python: Debug File" + F5
# script: /path/to/this/test/file.py

from pathlib import Path
from orcapod import AgentClient, Pod, Annotation, PodJob, OrcaPath
from asyncio import run

agent_client = AgentClient("test", "alpha")

test_dir = f"./tests/.tmp/{Path(__file__).stem}"

pod_jobs = [
    PodJob(
        annotation=Annotation(
            name="simple",
            description="This is an example pod job.",
            version=f"0.{i}.0",
        ),
        pod=Pod(
            annotation=Annotation(
                name="simple",
                description="This is an example pod.",
                version=f"{i}.0.0",
            ),
            image="alpine:3.14",
            command=f"sleep {i}",
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
        cpu_limit=0.1,
        memory_limit=10 << 20,
        env_vars=None,
        namespace_lookup={
            "default": f"{test_dir}/default",
        },
    )
    for i in range(1, 3)
]

run(agent_client.submit_pod_jobs(pod_jobs))
