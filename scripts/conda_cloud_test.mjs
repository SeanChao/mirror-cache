$`docker run --rm --network host \
	continuumio/miniconda3 bash -c "conda config --set custom_channels.pytorch http://localhost:9000/anaconda/cloud/ \
	&& conda install -c pytorch -y --download-only -v torchtext"
	`