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

with open("/input/image.jpeg", "rb") as f:
    image = f.read()

style_path = "/input/style.t7"

net = cv2.dnn.readNetFromTorch(style_path)
image = np.frombuffer(image, np.uint8)
image = cv2.imdecode(image, cv2.IMREAD_COLOR)
# image = cv2.cvtColor(image, cv2.COLOR_BGR2RGB)

image = imutils.resize(image, width=600)
(h, w) = image.shape[:2]

# construct a blob from the image, set the input, and then perform a
# forward pass of the network
blob = cv2.dnn.blobFromImage(image, 1.0, (w, h),
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

with open("/output/result.jpeg", "wb") as f:
    f.write(cv2.imencode('.jpeg', output)[1].tobytes())

print("done!")