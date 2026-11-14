"""
create_forms_domain.py (WLST Jython Script)
=====================================

Purpose:
    Offline WebLogic Scripting Tool (WLST) script to create and provision
    the standalone Oracle Forms 14c development domain (base_domain) with
    embedded AdminServer listening on port 7001.

Layer: Tooling / Skill Script (setting-up-oracle-testbed)

Usage Examples:
    podman exec -it forms-server-14c /oracle_home_14c/oracle_common/common/bin/wlst.sh create_forms_domain.py

Supported Object Types:
    - WebLogic 14.1.2 Base Domain (wls.jar)
    - Oracle Forms 14.1.2 Development Template (forms_development_template.jar)

CLI Arguments:
    None (Executed directly by WLST interpreter)

Input Files:
    - /oracle_home_14c/wlserver/common/templates/wls/wls.jar
    - /oracle_home_14c/forms/common/templates/wls/forms_development_template.jar

Output:
    - /oracle_home_14c/user_projects/domains/base_domain/ (Configured domain)

Key Functions:
    - readTemplate(templatePath)
    - addTemplate(templatePath)
    - writeDomain(domainPath)

Script Dependencies:
    - Oracle WebLogic WLST runtime (/oracle_home_14c/oracle_common/common/bin/wlst.sh)
    - Java 17+ (Oracle JDK 17)

Consumed by:
    - setting-up-oracle-testbed skill during initial domain provisioning
"""

import os
import sys

oracle_home = '/oracle_home_14c'
domain_dir = '/oracle_home_14c/user_projects/domains/base_domain'

print("=== 1. Reading Base WebLogic Server Template ===")
readTemplate(oracle_home + '/wlserver/common/templates/wls/wls.jar')

print("=== 2. Adding Oracle Forms Development Template ===")
addTemplate(oracle_home + '/forms/common/templates/wls/forms_development_template.jar')

if not os.environ.get('WLS_ADMIN_PASSWORD'):
    raise SystemExit('Set WLS_ADMIN_PASSWORD (for example: podman run --env-file .env ...) before running this script.')
print("=== 3. Setting Administrator Credentials ===")
cd('/Security/base_domain/User/weblogic')
cmo.setPassword(os.environ['WLS_ADMIN_PASSWORD'])

print("=== 4. Setting AdminServer Listen Port ===")
cd('/Server/AdminServer')
set('ListenAddress', '')
set('ListenPort', 7001)

print("=== 5. Writing Domain to Disk ===")
writeDomain(domain_dir)
closeTemplate()

print("=== Domain Creation Complete: " + domain_dir + " ===")
exit()
