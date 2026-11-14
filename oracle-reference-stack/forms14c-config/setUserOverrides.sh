#!/bin/sh
export LD_LIBRARY_PATH=/oracle_home_14c/lib:/oracle_home_14c/oracle_common/lib:${LD_LIBRARY_PATH}
export FORMS_PATH=/work/samples:/oracle_home_14c/forms
export FORMS_INSTANCE=/oracle_home_14c/user_projects/domains/base_domain/config/fmwconfig/components/FORMS/forms1
export ORACLE_PATH=/oracle_home_14c/forms:/oracle_home_14c/forms/admin/resource/US
export TNS_ADMIN=/oracle_home_14c/user_projects/domains/base_domain/config/fmwconfig
EXTRA_JAVA_PROPERTIES="-Dforms.instance=forms1 ${EXTRA_JAVA_PROPERTIES}"
export EXTRA_JAVA_PROPERTIES
