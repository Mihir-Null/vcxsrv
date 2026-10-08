"""Build corresponding-source ZIP from retained inputs and validated Cargo vendors."""
import argparse,contextlib,hashlib,io,json,pathlib,shutil,subprocess,tarfile,tempfile,tomllib,zipfile

@contextlib.contextmanager
def owned_vendor(repo,registry,crates):
 cache=registry.parent.parent/'cache'/registry.name
 archives={}
 for crate in crates:
  if crate['source']!='registry+https://github.com/rust-lang/crates.io-index':raise ValueError('Unsupported locked source')
  name=crate['name']+'-'+crate['version'];archive=cache/(name+'.crate')
  if archive.is_symlink() or not archive.is_file():raise ValueError('Locked cached package checksum mismatch: '+name)
  data=archive.read_bytes()
  if hashlib.sha256(data).hexdigest()!=crate['checksum']:raise ValueError('Locked cached package checksum mismatch: '+name)
  archives[name]=data
 parent=pathlib.Path(tempfile.gettempdir()).resolve()
 staging=pathlib.Path(tempfile.mkdtemp(prefix='vcxsrv-source-vendor-',dir=parent)).resolve()
 if staging.parent!=parent:raise ValueError('Owned vendor staging escaped temporary parent')
 try:
  vendor=staging/'vendor'
  subprocess.run(['cargo','vendor','--manifest-path',str((repo/'manager/Cargo.toml').resolve()),'--locked','--offline','--versioned-dirs',str(vendor)],cwd=repo/'manager',check=True,capture_output=True,text=True)
  if {d.name for d in vendor.iterdir()}!=set(archives):raise ValueError('Unexpected vendored package set')
  for crate in crates:
   name=crate['name']+'-'+crate['version'];directory=vendor/name
   checks=json.loads((directory/'.cargo-checksum.json').read_text(encoding='utf-8'))
   if checks['package']!=crate['checksum'] or not checks['files']:raise ValueError('Invalid Cargo vendor checksum metadata: '+name)
   actual={f.relative_to(directory).as_posix() for f in directory.rglob('*') if f.is_file() and f.name!='.cargo-checksum.json'}
   if actual!=set(checks['files']):raise ValueError('Unlisted or missing vendored source: '+name)
   with tarfile.open(fileobj=io.BytesIO(archives[name]),mode='r:gz') as archive:
    for relative,digest in checks['files'].items():
     path=pathlib.PurePosixPath(relative)
     if path.is_absolute() or '..' in path.parts:raise ValueError('Unsafe vendor checksum path')
     file=directory/relative
     if file.is_symlink():raise ValueError('Vendored symlink refused')
     data=file.read_bytes();member=archive.getmember(name+'/'+relative)
     if not member.isfile() or hashlib.sha256(data).hexdigest()!=digest or archive.extractfile(member).read()!=data:
      raise ValueError('Vendor source differs from locked published package: '+name+'/'+relative)
  yield vendor
 finally:
  # Only the exact newly created staging tree is eligible for recursive cleanup.
  if staging.is_symlink() or staging.resolve().parent!=parent:raise ValueError('Owned vendor cleanup path changed')
  shutil.rmtree(staging)

def main():
 p=argparse.ArgumentParser();p.add_argument('--source',required=True);p.add_argument('--workspace',required=True);p.add_argument('--server-revision',required=True);p.add_argument('--output',required=True);p.add_argument('--registry',required=True);a=p.parse_args()
 repo=pathlib.Path(a.source);workspace=pathlib.Path(a.workspace);registry=pathlib.Path(a.registry)
 def listed(*args):
  return subprocess.check_output(['git','-C',str(repo),*args]).decode().splitlines()
 def add(z,path,name):
  if path.is_symlink():raise ValueError('Source symlink refused: '+str(path))
  if not path.is_file():raise ValueError('Missing corresponding source: '+str(path))
  z.write(path,name)
 lock=tomllib.loads((repo/'manager/Cargo.lock').read_text());crates=[x for x in lock['package'] if 'source' in x]
 source_manifest={'crateInputs':'cargo vendor --locked --offline --versioned-dirs; cached package SHA and generated file hashes verified against Cargo.lock and original .crate bytes','serverRevision':a.server_revision,'serverInputs':'actual retained snapshot including reviewed production patch and generator substitutions','managerAndPackagingRevision':listed('rev-parse','HEAD')[0],'sourceDirty':bool(listed('status','--porcelain')),'crates':[],'missingSnapshotFromPinnedGit':[]}
 with owned_vendor(repo,registry,crates) as vendor, zipfile.ZipFile(a.output,'w',zipfile.ZIP_DEFLATED,compresslevel=6) as z:
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
   name=c['name']+'-'+c['version'];d=vendor/name
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

if __name__=="__main__":main()
