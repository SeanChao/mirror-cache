PKG_NAME=$1
MIRROR=$2
echo "install ${PKG_NAME} with mirror ${MIRROR}"
docker pull python
docker \
	run \
	--network host \
	--rm \
	python \
	python -m \
	pip download \
	-i ${MIRROR} \
	--disable-pip-version-check \
	--retries 0 \
	--no-cache-dir \
	-v \
	${PKG_NAME}
