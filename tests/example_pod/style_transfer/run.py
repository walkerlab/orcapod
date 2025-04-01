import os
from time import sleep
import cv2
import numpy as np
import imutils

if os.getenv("RAISE_ERROR", "false").upper() == "TRUE":
    raise Exception("Raising requested error...")

delay = int(os.getenv("DELAY", "0"))
print(f"Waiting for {delay}[s]...", flush=True)
sleep(delay)

image_path = "/input/subject.jpeg"
style_path_map = {
    "/input/style1.t7": "/output/result1.jpeg",
    "/extra_styles/style2.t7": "/output/result2.jpeg",
}

with open(image_path, "rb") as f:
    image = f.read()

for style_path, result_path in style_path_map.items():
    net = cv2.dnn.readNetFromTorch(style_path)
    prepared_image = np.frombuffer(image, np.uint8)
    prepared_image = cv2.imdecode(prepared_image, cv2.IMREAD_COLOR)

    prepared_image = imutils.resize(prepared_image, width=600)
    (h, w) = prepared_image.shape[:2]

    # construct a blob from the image, set the input, and then perform a
    # forward pass of the network
    blob = cv2.dnn.blobFromImage(prepared_image, 1.0, (w, h),
        (103.939, 116.779, 123.680), swapRB=False, crop=False)
    net.setInput(blob)
    output = net.forward()

    # reshape the output tensor, add back in the mean subtraction, and
    # then swap the channel ordering
    output = output.reshape((3, output.shape[2], output.shape[3]))
    output[0] += 103.939
    output[1] += 116.779
    output[2] += 123.680
    output = output.transpose(1, 2, 0)
    output = np.clip(output, 0, 255)
    output= output.astype('uint8')

    with open(result_path, "wb") as f:
        f.write(cv2.imencode('.jpeg', output)[1].tobytes())

print("done!")
