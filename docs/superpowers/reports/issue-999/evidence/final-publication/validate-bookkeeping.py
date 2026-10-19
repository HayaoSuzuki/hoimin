from pathlib import Path
import hashlib,json,re
root=Path.cwd();archive=root/"docs/superpowers/reports/issue-999";sdd=root/".superpowers/sdd/2026-10-19-issue-999-creusot-adoption"
listed=set()
for line in (archive/"SHA256SUMS").read_text().splitlines():
    digest,name=line.split("  ",1);file=archive/name
    assert file.is_file() and hashlib.sha256(file.read_bytes()).hexdigest()==digest,name
    listed.add(name)
assert listed=={str(p.relative_to(archive)) for p in archive.rglob("*") if p.is_file() and p.name!="SHA256SUMS"}
for name in ("final-review.md","progress.md"):
    assert (sdd/name).read_bytes()==(archive/"records"/name).read_bytes(),name
progress=(sdd/"progress.md").read_text();rulings=[line for line in progress.splitlines() if line.startswith("Ruling:")]
assert len(rulings)==7
line=next(line for line in progress.splitlines() if line.startswith("- Controller disposition of all six Declinedtojudge items:"))
assert line in (archive/"records/final-controller-dispositions.md").read_text().splitlines()
review=(archive/"records/final-review.md").read_text();declined=review.split("## Declined to judge",1)[1].split("## Recommendations",1)[0]
assert sum(line.startswith("- **") for line in declined.splitlines())==6
observation=archive/"evidence/final-publication/branch-protection-observation.json"
assert observation.read_bytes()==(sdd/"final-publication-evidence/branch-protection-observation.json").read_bytes()
data=json.loads(observation.read_text());assert data["branch"]["protected"] is True
assert {item["type"] for item in data["effective_rules"]}=={"deletion","non_fast_forward"}
assert data["branch"]["protection"]["required_status_checks"]=={"checks":[],"contexts":[],"enforcement_level":"off"}
for name in ["docs/superpowers/reports/2026-10-19-issue-999-creusot-adoption.md","docs/superpowers/reviews/2026-10-19-issue-999-creusot-adoption.md","docs/superpowers/plans/2026-10-19-issue-999-creusot-adoption.md","docs/superpowers/specs/2026-10-19-issue-999-creusot-adoption-design.md","formal/creusot/README.md"]:
    page=root/name
    for target in re.findall(r"\]\(([^\s)]+)",page.read_text()):
        if "://" in target or target.startswith("#"):continue
        assert (page.parent/target.split("#",1)[0]).exists(),(name,target)
print(json.dumps({"archive_files_verified":len(listed),"rulings_preserved":7,"declined_items_disposed":6,"final_review_and_ledger_byte_identical":True,"api_observation_byte_identical":True,"changed_canonical_file_links_valid":True},indent=2))
