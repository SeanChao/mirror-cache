# Configuration example

The example [configuration file](../config.yml).

You may try it out:

```sh
pip install -i http://localhost:9000 requests
conda install -c http://localhost:9000 requests
conda config --set custom_channels.pytorch http://localhost:9000/anaconda/cloud/ && conda install -c pytorch -y --download-only -v torchtext

# Ubuntu
# In /etc/apt/sources.list: change links like http://xxx.ubuntu.com/ubuntu into http://localhost:9000/ubuntu
apt-get update
```
