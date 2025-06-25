#!/usr/bin/env python3

# build: maturin develop --uv
# debugger: select "Python: Debug File" + F5
# script: /path/to/this/test/file.py /path/to/directory (must exist, will create subdirectory)
import shutil
from pathlib import Path
import argparse
import asyncio
from orcapod import (
    Pod,
    PodJob,
    Annotation,
    Uri,
    LocalDockerOrchestrator,
    LocalFileStore,
    ModelId,
    ModelType,
    OrcaError,
)


def create_pod(data, _):
    data["pod"] = Pod(
        annotation=Annotation(
            name="simple",
            description="This is an example pod.",
            version="1.0.0",
        ),
        image="alpine:3.14",
        command="sleep 1",
        input_spec={},
        output_dir="/tmp/output",
        output_spec={},
        source_commit_url="https://github.com/user/simple",
        recommended_cpus=0.1,
        recommended_memory=10 << 20,
        required_gpu=None,
    )
    return data["pod"], data


def create_pod_job(data, config):
    data["pod_job"] = PodJob(
        annotation=Annotation(
            name="simple",
            description="This is an example pod job.",
            version="0.1.0",
        ),
        pod=data["pod"],
        input_packet={},
        output_dir=Uri(
            namespace="default",
            path=".",
        ),
        cpu_limit=0.1,
        memory_limit=10 << 20,
        env_vars=None,
        namespace_lookup=config["namespace_lookup"],
    )
    return data["pod_job"], data


def create_orch(data, _):
    data["orch"] = LocalDockerOrchestrator()
    return data["orch"], data


def start_pod_job(data, config):
    data["pod_run"] = data["orch"].start_blocking(
        namespace_lookup=config["namespace_lookup"], pod_job=data["pod_job"]
    )
    return data["pod_run"], data


def wait_for_pod_result(data, _):
    print([str(p) for p in asyncio.run(data["orch"].list())])
    print("waiting to finish...")
    data["pod_result"] = data["orch"].get_result_blocking(pod_run=data["pod_run"])
    return data["pod_result"], data


def delete_pod_run(data, _):
    print([str(p) for p in data["orch"].list_blocking()])
    data["orch"].delete_blocking(pod_run=data["pod_run"])
    return [str(p) for p in data["orch"].list_blocking()], data


def create_store(data, config):
    data["store"] = LocalFileStore(directory=config["store_dir"])
    return data["store"], data


def save_pod(data, _):
    data["store"].save_pod(data["pod"])
    return [str(p) for p in data["store"].list_pod()], data


def load_pod(data, _):
    data["loaded_pod"] = data["store"].load_pod(
        model_id=ModelId.ANNOTATION(
            data["pod"].annotation().name, data["pod"].annotation().version
        )
    )
    return data["loaded_pod"], data


def delete_annotation(data, _):
    data["store"].delete_annotation(
        model_type=ModelType.POD,
        name=data["pod"].annotation().name,
        version=data["pod"].annotation().version,
    )
    return [str(p) for p in data["store"].list_pod()], data


def delete_pod(data, _):
    data["store"].delete_pod(model_id=ModelId.HASH(data["pod"].hash()))
    return [str(p) for p in data["store"].list_pod()], data


def test(test_dir, steps):
    config = {
        "namespace_lookup": {
            "default": f"{test_dir}/default",
        },
        "store_dir": f"{test_dir}/store",
    }
    data = {}
    try:
        for step in steps:
            print(f"\n==================== {step.__name__} ====================\n")
            result, data = step(data, config)
            print(result)
    except OrcaError as e:
        if "No such file or directory (os error 2)" not in str(e):
            raise e
    finally:
        shutil.rmtree(test_dir)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Basic tests for orcapod's Python API."
    )
    parser.add_argument("test_dir", help="Root directory for tests.")
    args = parser.parse_args()

    test(
        test_dir=f"{args.test_dir}/{Path(__file__).stem}",
        steps=[
            # Orchestrator DEMO
            create_pod,
            create_pod_job,
            create_orch,
            start_pod_job,
            wait_for_pod_result,
            delete_pod_run,
            # Store DEMO
            create_store,
            save_pod,
            load_pod,
            delete_annotation,
            delete_pod,
            delete_pod,  # force error to check output
        ],
    )
