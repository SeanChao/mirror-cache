PKG_NAME=$1
MIRROR=$2
echo "install ${PKG_NAME} with mirror ${MIRROR}"
python -m pip uninstall -y ${PKG_NAME} && python -m pip install --retries 0 --no-cache-dir -v -i http://localhost:9000/pypi/web/simple ${PKG_NAME}
