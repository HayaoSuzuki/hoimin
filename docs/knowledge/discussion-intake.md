---
type: Playbook
title: Discussion intake and activation
description: Maintain report forms and activate discussion categories without opening external Issues or pull requests.
status: draft
catalog_revision: 1b2288e29223292939b313af1809d5c2ebebab7d
sources:
- id: support
  resource: ../support.md
  revision: 34723ff3f9b19c28140a6efad0651e542e25b8c1
  working_tree: modified
  sha256: 3900987719f8c5b2846e19c5d892d96ac1e053a773c7d446ce7b49b45534fcf1
---

# Discussion intake and activation

Use the [reporting guide](../support.md) for the intake policy, minimal-reproduction
advice and the linked maintainer procedure. Maintainers track accepted work in Issues; external
Issue/PR submissions remain closed.[^support]

Bug reports and Q&A use static category forms. Keep category slugs and template
filenames aligned. Discussions is enabled; publish template changes through a
work branch/PR and check the category forms without submitting posts. Restoration
and maintenance instructions live in the development guide linked from the
reporting guide.[^support]

The templates request small public excerpts and add no automatic fetching or
execution of submissions. Check YAML and links locally; category matching and
GitHub rendering need confirmation after activation. Recheck this procedure when
changing categories, intake policy or form fields.[^support]

[^support]: [Questions and bug reports](../support.md).
