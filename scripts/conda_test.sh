PKG_NAME=$1
MIRROR=$2

docker \
	run \
	--rm \
	--network host \
	continuumio/miniconda3 conda \
	install \
	--download-only \
	-v \
	-y \
	-c ${MIRROR} \
	${PKG_NAME}
