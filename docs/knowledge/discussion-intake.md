---
type: Playbook
title: Discussion intake and activation
description: Maintain report forms and activate discussion categories without opening external Issues or pull requests.
status: draft
catalog_revision: 1b2288e29223292939b313af1809d5c2ebebab7d
sources:
- id: support
  resource: ../support.md
  revision: 1b2288e29223292939b313af1809d5c2ebebab7d
  working_tree: untracked
  sha256: 71c37c38f8bc61c3e45e32139b39ed4e960b1fd37cb503fd68242c33e56ad212
---

# Discussion intake and activation

Use the [reporting guide](../support.md) for the intake policy, minimal-reproduction
advice and post-merge setup. Maintainers track accepted work in Issues; external
Issue/PR submissions remain closed.[^support]

Bug reports and Q&A use static category forms. Keep category slugs and template
filenames aligned. Publish templates through a work branch/PR before enabling
Discussions and checking the category forms without submitting posts.[^support]

The templates request small public excerpts and add no automatic fetching or
execution of submissions. Check YAML and links locally; category matching and
GitHub rendering need confirmation after activation. Recheck this procedure when
changing categories, intake policy or form fields.[^support]

[^support]: [Questions and bug reports](../support.md).
