/**
 * RapidRAW Cloud, switched off. Ours.
 *
 * RapidRAW 1.6.5 sells remote generative edits through an account with
 * RapidRAW: a Clerk sign-in, a subscription, and requests to getrapidraw.com.
 * Argentum has no account, takes no money in another author's name, and does
 * not contact a server it was never asked to.
 *
 * Their `AppWrapper` signs in to Clerk on every launch, before anyone has
 * chosen Cloud - it registers the app with Clerk's servers and loads the
 * sign-in screens from the web. Their store has a state for a platform where
 * Cloud cannot run, 'unsupported', and `initAuth` then returns before touching
 * the network. This sets it for every platform, at import, which is before any
 * effect of theirs can run.
 *
 * Settings offers no Cloud tile (SettingsPanel.tsx's provider list), and a
 * provider saved as "cloud" reads as the built-in AI (app_settings.rs,
 * load_settings). The rest is in the build files: the Clerk plugin and its
 * permissions are not built in, and the HTTP permission allows no address.
 */

import { useCloudStore } from '../store/useCloudStore';

useCloudStore.setState({ authStatus: 'unsupported' });
