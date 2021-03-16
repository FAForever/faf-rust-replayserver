Usage
=====

Deployment
----------

TODO: integrate into faf-stack.

The server is built with ``cargo build``, its executable is
``faf_rust_replayserver``. For deployment, you'll want to use the provided
Dockerfile, or better yet, a github release. The server is integrated into
`faf-stack <https://github.com/FAForever/faf-stack>`_.

To release a new version, just tag the commit and prepare a release on Github.
A release Github action should do the rest.

Configuration
-------------

The server uses a yaml configuration file for almost all configuration. The path
to the configuration file is provided in an ``RS_CONFIG_FILE`` environment
variable. A full example configuration file with documentation is listed below.

.. literalinclude:: documented_config.yml
   :language: YAML

Note that the config file is similar to, but not compatible with the python
replay server's config file.

Two settings are configured via environment variables: database password and
log level. The database password should be provided in a ``RS_DB_PASSWORD``
variable, while the log level is set via the ``RUST_LOG`` variable.
``RUST_LOG`` can take one of values "off", "error", "warn", "info", "debug",
"trace".

The server does not accept any commandline arguments. All logging is done to
stderr.
