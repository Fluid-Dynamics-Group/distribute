paper: 
	sudo podman run --rm \
		--volume $PWD/paper:/data \
		--user $(id -u):$(id -g) \
		--env JOURNAL=joss \
		docker.io/openjournals/inara

paper-docker: 
	sudo docker run --rm \
		--volume $PWD/paper:/data \
		--user $(id -u):$(id -g) \
		--env JOURNAL=joss \
		openjournals/inara

book:
	mdbook watch docs/ --open 
