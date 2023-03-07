paper: 
	sudo docker run --rm \
		--volume $PWD/paper:/data \
		--user $(id -u):$(id -g) \
		--env JOURNAL=joss \
		openjournals/inara

book:
	mdbook watch docs/ --open 
