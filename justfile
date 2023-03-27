paper: 
	sudo podman run --rm \
		--volume $PWD/paper:/data \
		--user $(id -u):$(id -g) \
		--env JOURNAL=joss \
		docker.io/openjournals/inara

book:
	mdbook watch docs/ --open 
