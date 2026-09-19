from pathlib import Path
import json,shutil,subprocess,xml.etree.ElementTree as E
R=Path(__file__).parent/'overlap-roundtrip';R.mkdir(exist_ok=True);C=Path('/Users/murzav/projects/rust/xcstrings-mcp-pr26/tests/fixtures/apple_xcode27');N='urn:oasis:names:tc:xliff:document:1.2';E.register_namespace('',N)
original=json.loads((C/'positive/catalog-matrix/source.xcstrings').read_text())
su=lambda v:{'stringUnit':{'state':'translated','value':v}}
overlap={**su('%lld root'),'variations':{'plural':{'one':su('%lld one'),'other':su('%lld many')}}}
axes={'variations':{'plural':{'one':su('%lld one'),'other':su('%lld many')},'device':{'iphone':su('%lld phone'),'other':su('%lld device')}}}
base={'sourceLanguage':'en','version':'1.0','strings':{'unit_and_variations':{'extractionState':'manual','localizations':{'en':overlap,'fr':overlap}},'plural_and_device':{'extractionState':'manual','localizations':{'en':axes,'fr':axes}}}}
def run(d,stage,cmd):
 p=subprocess.run(list(map(str,cmd)),capture_output=True,text=True);(d/(stage+'.log')).write_text(repr(list(map(str,cmd)))+'\nEXIT: '+str(p.returncode)+'\n'+p.stdout+p.stderr);return p.returncode
for mode in ['existing']:
 d=R/mode;d.mkdir(exist_ok=True);pr=d/'Oracle.xcodeproj';pr.mkdir(exist_ok=True);shutil.copyfile(C/'provenance/single-catalog-project.pbxproj',pr/'project.pbxproj');cat=d/'Localizable.xcstrings';cat.write_text(json.dumps(base,ensure_ascii=False));shutil.copyfile(cat,d/'before.xcstrings')
 run(d,'export',['xcodebuild','-exportLocalizations','-project',pr,'-localizationPath',d/'export','-exportLanguage','fr']);xp=d/'export/fr.xcloc/Localized Contents/fr.xliff';shutil.copyfile(xp,d/'actual-export.xliff');t=E.parse(xp)
 for u in t.findall('.//{'+N+'}trans-unit'):
  target=u.find('{'+N+'}target');target.text=(target.text or '')+' CHANGED'
 if mode!='existing':
  body=t.find('.//{'+N+'}body');u=E.SubElement(body,'{'+N+'}trans-unit',{'id':'target_only_substitution|==|substitutions.COUNT.plural.many','{http://www.w3.org/XML/1998/namespace}space':'preserve'});E.SubElement(u,'{'+N+'}source').text='target_only_substitution|==|substitutions.COUNT.plural.'+('many' if mode=='many-own-id' else 'other');E.SubElement(u,'{'+N+'}target',{'state':'translated'}).text='fr %1$lld NEWMANY'
 edited=d/'edited-input.xliff';t.write(edited,encoding='utf-8',xml_declaration=True);edited.write_bytes(edited.read_bytes().replace(b'\r',b'&#13;'))
 run(d,'import',['xcodebuild','-importLocalizations','-project',pr,'-localizationPath',edited]);after=json.loads(cat.read_text());shutil.copyfile(cat,d/'after.xcstrings');run(d,'compile',['xcrun','xcstringstool','compile',cat,'--output-directory',d/'compiled']);run(d,'reexport',['xcodebuild','-exportLocalizations','-project',pr,'-localizationPath',d/'reexport','-exportLanguage','fr'])
 print(mode,json.dumps(after,ensure_ascii=False))
