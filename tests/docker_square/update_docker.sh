# throw errors
set -e

# build the new image
docker build . -t docker_square

# tag the image with username
docker image tag docker_square vanillabrooks/docker_square:latest

# in order to push, I logged into my account with
# docker login -u vanillabrooks
# but I omit that from this script

# push the image to dockerhub
docker image push vanillabrooks/docker_square:latest
