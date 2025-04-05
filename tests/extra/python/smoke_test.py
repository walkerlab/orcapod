# build: maturin develop --uv
# debugger: F5 on this file
# script: python tests/extra/python/smoke_test.py
import shutil
from orcapod import (
    Pod,
    PodJob,
    Annotation,
    OrcaPath,
    LocalDockerOrchestrator,
    LocalFileStore,
    ModelId,
    Model,
)


def create_pod(data, _):
    data["pod"] = Pod(
        annotation=Annotation(
            name="simple",
            description="This is an example pod.",
            version="1.0.0",
        ),
        image="alpine:3.14",
        # command=r"sh -c 'sleep 5 && echo finished...'", # needs arg parsing before will work
        command="sleep 1",
        input_stream={},
        output_dir="/tmp/output",
        output_stream={},
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
        input_stream={},
        output_dir=OrcaPath(
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
    print([str(p) for p in data["orch"].list_blocking()])
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
        model_id=ModelId.ANNOTATION("simple", "1.0.0")
    )
    return data["loaded_pod"], data


def delete_annotation(data, _):
    data["store"].delete_annotation(
        model_kind=Model.POD, name="simple", version="1.0.0"
    )
    return [str(p) for p in data["store"].list_pod()], data


def delete_pod(data, _):
    data["store"].delete_pod(
        model_id=ModelId.HASH(
            "e3bae432ae5b9f0d391a778234b2e14afae8e8b3abd2a09063455506f621014d"
        )
    )
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
    except Exception as e:
        raise e
    finally:
        shutil.rmtree(test_dir)


if __name__ == "__main__":
    test(
        test_dir="./tests/.tmp/smoke_test",
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
