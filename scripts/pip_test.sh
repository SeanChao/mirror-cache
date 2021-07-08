PKG_NAME=$1
MIRROR=$2
echo "install ${PKG_NAME} with mirror ${MIRROR}"
docker pull python
docker \
	run \
	--network host \
	python \
	python -m \
	pip install \
	-i http://localhost:9000/pypi/web/simple \
	--disable-pip-version-check \
	--retries 0 \
	--no-cache-dir \
	-v \
	${PKG_NAME}
