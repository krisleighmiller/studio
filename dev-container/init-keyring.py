#!/usr/bin/python3
"""Initialize/unlock the container's own default collection using native prompts.

No password input, secret values, or plaintext fallback is implemented here.
"""
import sys
import gi

gi.require_version('Secret', '1')
from gi.repository import Secret  # noqa: E402

try:
    service = Secret.Service.get_sync(Secret.ServiceFlags.OPEN_SESSION, None)
    collection = Secret.Collection.for_alias_sync(
        service, 'default', Secret.CollectionFlags.NONE, None)
    if collection is None:
        print('Studio: create a password-protected container keyring in the native prompt. '
              'Choose a non-empty password; do not use unencrypted storage.', flush=True)
        collection = Secret.Collection.create_sync(
            service, 'Studio Docker', 'default', Secret.CollectionCreateFlags.NONE, None)
    if collection is None:
        raise RuntimeError('Keyring creation was cancelled; Studio was not started.')
    if collection.get_locked():
        print('Studio: unlock the container keyring in the native prompt.', flush=True)
        service.unlock_sync([collection], None)
        collection.refresh()
    if collection.get_locked():
        raise RuntimeError('Keyring is still locked; Studio was not started.')
    # Do not accept the volatile session collection as persistent storage.
    if collection.get_object_path().endswith('/session'):
        raise RuntimeError('Default collection is session-only; persistent keyring required.')
    # This launcher deliberately uses GNOME Keyring. Its native binary signature
    # distinguishes encrypted files from its unencrypted textual keyring format.
    # Read only format headers, never secret-bearing file bodies.
    from pathlib import Path
    import os
    files = list((Path(os.environ['XDG_DATA_HOME']) / 'keyrings').glob('*.keyring'))
    if not files:
        raise RuntimeError('Persistent encrypted keyring file was not created.')
    for path in files:
        with path.open('rb') as stream:
            if stream.read(16) != b'GnomeKeyring\n\r\0\n':
                raise RuntimeError('Unencrypted keyring refused. Recreate this container-only '
                                   'keyring with a non-empty password before saving API keys.')
        path.chmod(0o600)
    print('Studio: default encrypted Secret Service collection is ready (no secrets read).', flush=True)
except Exception as error:
    print(f'Studio keyring setup failed: {error}', file=sys.stderr)
    sys.exit(1)
