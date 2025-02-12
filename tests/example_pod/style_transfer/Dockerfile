# docker build ./tests/example_pod/style_transfer -t example.server.com/user/style-transfer:1.0.0
# docker run --rm -d \
#   -l orcapod=true \
#   -l orcapod.pod_job.model_id.type=annotation \
#   -l orcapod.pod_job.model_id.name=style-transfer \
#   -l orcapod.pod_job.model_id.version=0.1.0 \
#   --entrypoint python \
#   -e DELAY=60 \
#   -v ./tests/data/images/dog.jpeg:/input/image.jpeg:ro \
#   -v ./tests/data/styles/mosaic.t7:/input/style.t7:ro \
#   -v ./tests/data/output:/output \
#   --cpus=0.5 \
#   --memory=2g \
#   example.server.com/user/style-transfer:1.0.0 \
#   /run.py
# docker ps -f 'label=org.orcapod=true'
FROM python:3.9-slim
RUN pip install opencv-python-headless numpy imutils
COPY ./run.py /run.py
CMD ["python", "/run.py"]