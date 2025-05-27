#!/usr/bin/env python3

# build: maturin develop --uv
# debugger: select "Python: Debug File" + F5
# script: /path/to/this/test/file.py

from pathlib import Path
from orcapod import AgentClient, Pod, Annotation, PodJob, OrcaPath
from asyncio import run

agent_client = AgentClient("test", "alpha")

run(agent_client.log("banana", "Does this work still?"))

test_dir = f"./tests/.tmp/{Path(__file__).stem}"

pod = Pod(
    annotation=Annotation(
        name="simple",
        description="This is an example pod.",
        version="1.0.0",
    ),
    image="alpine:3.14",
    command="sleep 1",
    input_stream={},
    output_dir="/tmp/output",
    output_stream={},
    source_commit_url="https://github.com/user/simple",
    recommended_cpus=0.1,
    recommended_memory=10 << 20,
    required_gpu=None,
)

pod_job = PodJob(
    annotation=Annotation(
        name="simple",
        description="This is an example pod job.",
        version="0.1.0",
    ),
    pod=pod,
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

run(agent_client.write_topic_data("/banana", pod_job))
