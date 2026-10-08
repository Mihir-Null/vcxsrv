"""Build complete corresponding-source ZIP from pinned paths and actual retained inputs."""
import argparse,hashlib,json,pathlib,subprocess,tomllib,zipfile
p=argparse.ArgumentParser();p.add_argument('--source',required=True);p.add_argument('--workspace',required=True);p.add_argument('--server-revision',required=True);p.add_argument('--output',required=True);p.add_argument('--registry',required=True);a=p.parse_args()
repo=pathlib.Path(a.source);workspace=pathlib.Path(a.workspace);registry=pathlib.Path(a.registry)
def listed(*args):
 return subprocess.check_output(['git','-C',str(repo),*args]).decode().splitlines()
def add(z,path,name):
 if path.is_symlink():raise ValueError('Source symlink refused: '+str(path))
 if not path.is_file():raise ValueError('Missing corresponding source: '+str(path))
 z.write(path,name)
lock=tomllib.loads((repo/'manager/Cargo.lock').read_text());crates=[x for x in lock['package'] if 'source' in x]
source_manifest={'serverRevision':a.server_revision,'serverInputs':'actual retained snapshot including reviewed production patch and generator substitutions','managerAndPackagingRevision':listed('rev-parse','HEAD')[0],'sourceDirty':bool(listed('status','--porcelain')),'crates':[],'missingSnapshotFromPinnedGit':[]}
with zipfile.ZipFile(a.output,'w',zipfile.ZIP_DEFLATED,compresslevel=6) as z:
 for rel in listed('ls-tree','-r','--name-only',a.server_revision):
  path=workspace/rel
  if path.exists():add(z,path,'server/'+rel)
  elif rel.startswith(('tests/','docs/')):
   data=subprocess.check_output(['git','-C',str(repo),'show',a.server_revision+':'+rel]);z.writestr('server/'+rel,data)
   source_manifest['missingSnapshotFromPinnedGit'].append({'path':rel,'sha256':hashlib.sha256(data).hexdigest(),'revision':a.server_revision})
  else:raise ValueError('Missing retained production/build source: '+rel)
 for rel in listed('ls-files'):
  if rel.startswith(('manager/','packaging/','integration/','docs/','tests/')):add(z,repo/rel,'product/'+rel)
 for c in crates:
  name=c['name']+'-'+c['version'];d=registry/name
  if not d.is_dir():raise ValueError('Locked crate source not cached: '+name)
  notices=[]
  for f in sorted(d.rglob('*')):
   if f.is_file():
    add(z,f,'product/manager/vendor/'+name+'/'+f.relative_to(d).as_posix())
    if f.name.upper().startswith(('LICENSE','COPYING','NOTICE')):notices.append(f.relative_to(d).as_posix())
  metadata=tomllib.loads((d/'Cargo.toml').read_text()).get('package',{})
  source_manifest['crates'].append({'name':c['name'],'version':c['version'],'checksum':c['checksum'],'license':metadata.get('license'),'notices':notices})
 z.writestr('product/manager/.cargo/config.toml','[source.crates-io]\nreplace-with = "vendored-sources"\n[source.vendored-sources]\ndirectory = "vendor"\n')
 z.writestr('SOURCE-INVENTORY.json',json.dumps(source_manifest,indent=2))
print(json.dumps({'archive':a.output,'bytes':pathlib.Path(a.output).stat().st_size,'sha256':hashlib.sha256(pathlib.Path(a.output).read_bytes()).hexdigest(),'crates':source_manifest['crates']}))
