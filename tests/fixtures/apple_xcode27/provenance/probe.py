from pathlib import Path
import json, shutil, subprocess, xml.etree.ElementTree as E, hashlib
R=Path(__file__).parent
P=R.parent
shutil.copytree(P/'Oracle.xcodeproj', R/'Oracle.xcodeproj', dirs_exist_ok=True)
NS={'x':'urn:oasis:names:tc:xliff:document:1.2'}
E.register_namespace('',NS['x'])
def su(s,state='translated'): return {'stringUnit':{'state':state,'value':s}}
def entry(en,fr=None): return {'extractionState':'manual','localizations':{'en':en,**({'fr':fr} if fr else {})}}
def write(p,v): p.write_text(json.dumps(v,ensure_ascii=False,indent=2)+'\n')
def run(name,cmd):
 with (R/(name+'.log')).open('w') as f:
  f.write('COMMAND: '+repr(cmd)+'\n'); f.flush(); q=subprocess.run(cmd,stdout=f,stderr=subprocess.STDOUT)
 return q.returncode
def stage(name,data,mutate=None):
 d=R/name;d.mkdir(exist_ok=True);write(d/'before.xcstrings',data);write(R/'Localizable.xcstrings',data)
 codes={}; codes['compile']=run(name+'-compile',['xcrun','xcstringstool','compile',str(R/'Localizable.xcstrings'),'--output-directory',str(d/'compiled')])
 codes['export']=run(name+'-export',['xcodebuild','-exportLocalizations','-project',str(R/'Oracle.xcodeproj'),'-localizationPath',str(d/'export'),'-exportLanguage','fr'])
 if codes['export']==0:
  x=d/'export/fr.xcloc/Localized Contents/fr.xliff';shutil.copy2(x,d/'before.xliff')
  if mutate: mutate(x)
  tree=E.parse(x); units=[{'file':f.attrib,'units':[{'attrs':u.attrib,'source':u.findtext('x:source',namespaces=NS),'target':u.findtext('x:target',namespaces=NS),'target_attrs':(u.find('x:target',NS).attrib if u.find('x:target',NS) is not None else None),'notes':[{'attrs':n.attrib,'text':n.text} for n in u.findall('x:note',NS)]} for u in f.findall('.//x:trans-unit',NS)]} for f in tree.findall('x:file',NS)];write(d/'units.json',units)
  codes['import']=run(name+'-import',['xcodebuild','-importLocalizations','-project',str(R/'Oracle.xcodeproj'),'-localizationPath',str(x)])
  after=json.loads((R/'Localizable.xcstrings').read_text());write(d/'after.xcstrings',after)
  codes['semantic_equal']=data==after
  codes['changed_keys']=[k for k in set(data['strings'])|set(after['strings']) if data['strings'].get(k)!=after['strings'].get(k)]
  codes['after_compile']=run(name+'-after-compile',['xcrun','xcstringstool','compile',str(R/'Localizable.xcstrings'),'--output-directory',str(d/'compiled-after')])
  codes['reexport']=run(name+'-reexport',['xcodebuild','-exportLocalizations','-project',str(R/'Oracle.xcodeproj'),'-localizationPath',str(d/'reexport'),'-exportLanguage','fr'])
  for folder in ['compiled','compiled-after']: shutil.rmtree(d/folder,ignore_errors=True)
 write(d/'results.json',codes);print(name,codes,flush=True)
 return codes
if __name__=='__main__':
 states=['new','translated','needs_review','stale','unknown']
 data={'sourceLanguage':'en','strings':{s:entry(su('source '+s),su('target '+s,s)) for s in states},'version':'1.0'}
 stage('states-export',data)
 base={'sourceLanguage':'en','strings':{},'version':'1.0'}
 options={'translated':{'state':'translated'},'new':{'state':'new'},'needs-review-l10n':{'state':'needs-review-l10n'},'needs-review-translation':{'state':'needs-review-translation'},'final':{'state':'final'},'signed-off':{'state':'signed-off'},'unknown':{'state':'custom'},'qualifier':{'state':'translated','state-qualifier':'needs-review-l10n'},'no-state':{},'empty':{'state':'translated'},'missing':{'state':'translated'}}
 for k in options: base['strings'][k]=entry(su('source '+k),su('old '+k))
 def mutate(x):
  tree=E.parse(x)
  for u in tree.findall('.//x:trans-unit',NS):
   k=u.attrib['id'];t=u.find('x:target',NS);t.attrib.clear();t.attrib.update(options[k]);t.text='changed '+k
   if k=='empty': t.text=''
   if k=='missing':u.remove(t)
  tree.write(x,encoding='utf-8',xml_declaration=True)
 stage('states-import',base,mutate)
 names=json.loads((P/'names-source.xcstrings').read_text())
 subset={'sourceLanguage':'en','strings':{k:names['strings'][k] for k in ['target_only_substitution','name_A..B','name_é','sub_DOT.NAME'] if k in names['strings']},'version':'1.0'}
 stage('target-substitution',subset)
 controls={'sourceLanguage':'en','strings':{k:entry(su('source '+k),su('target '+k)) for k in ['key\rCR','key\nLF','key\tTAB','clé漢字🙂','<>&"quote']},'version':'1.0'}
 stage('controls',controls)
