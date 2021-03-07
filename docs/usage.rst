Usage
=====

Deployment
----------

TODO: actually make the Dockerfile and integrate into faf-stack.

The server is built with ``cargo build``, its executable is
``faf_rust_replayserver``. For deployment, you'll want to use the provided
Dockerfile. The server is integrated into `faf-stack
<https://github.com/FAForever/faf-stack>`_.

Configuration
-------------

The server uses a yaml configuration file for almost all configuration. The path
to the configuration file is provided in an ``RS_CONFIG_FILE`` environment
variable. Additionally, the password to FAF database is also provided in the
environment variable ``RS_DB_PASSWORD``. A full example configuration
file with documentation is listed below.

.. literalinclude:: documented_config.yml
   :language: YAML

Note that the config file is similar to, but not compatible with the python
replay server's config file.

The server does not accept any commandline arguments. All logging is done to
stderr.
